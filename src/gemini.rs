use anyhow::{Context, Result, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::prompt::SYSTEM_INSTRUCTION;

const FALLBACK_MODELS: &[&str] = &[
    "gemini-3.5-flash-lite",
    "gemini-3.5-flash",
    "gemini-3.8-flash",
    "gemini-3.6-flash",
    "gemini-flash-latest",
];

#[derive(Debug, Clone)]
pub struct ListingRequest {
    pub image: Option<PathBuf>,
    pub url: Option<String>,
    pub info: Option<String>,
}

impl ListingRequest {
    /// Garante programaticamente que há ao menos uma referência válida
    pub fn validate(&self) -> Result<()> {
        let has_image = self.image.is_some();
        let has_url = self.url.as_deref().is_some_and(|s| !s.trim().is_empty());
        let has_info = self.info.as_deref().is_some_and(|s| !s.trim().is_empty());

        if !has_image && !has_url && !has_info {
            bail!("Envie ao menos uma referência (imagem, URL ou metadados)!");
        }
        Ok(())
    }

    /// Monta o payload `parts` para a API REST do Gemini
    fn build_gemini_parts(&self) -> Result<Vec<serde_json::Value>> {
        self.validate()?;
        let mut parts = Vec::new();

        // 1. Texto (metadados + URL de referência)
        let mut text_blocks = Vec::new();

        if let Some(info) = &self.info
            && !info.trim().is_empty()
        {
            text_blocks.push(format!(
                "Metadados e especificações da peça:\n{}",
                info.trim()
            ));
        }

        if let Some(url) = &self.url
            && !url.trim().is_empty()
        {
            text_blocks.push(format!(
                "URL de referência do modelo 3D (extraia nome do personagem/obra do link se necessário):\n{}",
                url.trim()
            ));
        }

        if text_blocks.is_empty() {
            text_blocks.push(
                "Analise exclusivamente a imagem fornecida para identificar o personagem, universo e acabamento."
                    .to_string(),
            );
        }

        parts.push(json!({ "text": text_blocks.join("\n\n") }));

        // 2. Visual (se houver imagem)
        if let Some(image_path) = &self.image {
            let bytes = fs::read(image_path).with_context(|| {
                format!("Não foi possível ler a imagem: {}", image_path.display())
            })?;
            let b64 = STANDARD.encode(&bytes);
            let mime_type = detect_mime_type(image_path)?;

            parts.push(json!({
                "inline_data": {
                    "mime_type": mime_type,
                    "data": b64
                }
            }));
        }

        Ok(parts)
    }
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct ShopeeListing {
    #[serde(alias = "title")]
    pub titulo: String,
    #[serde(alias = "description")]
    pub descricao: String,
    #[serde(skip)]
    pub model_used: String,
}

#[derive(Deserialize)]
struct GeminiResponse {
    candidates: Option<Vec<Candidate>>,
}

#[derive(Deserialize)]
struct Candidate {
    content: Content,
}

#[derive(Deserialize)]
struct Content {
    parts: Vec<Part>,
}

#[derive(Deserialize)]
struct Part {
    text: String,
}

fn detect_mime_type(path: &Path) -> Result<&'static str> {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|s| s.to_lowercase())
        .as_deref()
    {
        Some("jpg" | "jpeg") => Ok("image/jpeg"),
        Some("png") => Ok("image/png"),
        Some("webp") => Ok("image/webp"),
        _ => bail!("Formato de imagem não suportado. Use PNG, JPG ou WEBP."),
    }
}

fn clean_markdown_json(raw: &str) -> &str {
    let trimmed = raw.trim();
    let without_prefix = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .unwrap_or(trimmed);
    without_prefix
        .strip_suffix("```")
        .unwrap_or(without_prefix)
        .trim()
}

pub async fn generate_listing(api_key: &str, req: &ListingRequest) -> Result<ShopeeListing> {
    let parts = req.build_gemini_parts()?;

    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .context("Falha ao inicializar cliente HTTP")?;

    let payload = json!({
        "system_instruction": {
            "parts": [{ "text": SYSTEM_INSTRUCTION }]
        },
        "contents": [{
            "parts": parts
        }],
        "generationConfig": {
            "responseMimeType": "application/json"
        }
    });

    let mut last_error = anyhow::anyhow!("Nenhum modelo disponível na lista de fallback");

    for &model in FALLBACK_MODELS {
        let endpoint = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent"
        );

        let response = match client
            .post(&endpoint)
            .header("x-goog-api-key", api_key)
            .json(&payload)
            .send()
            .await
        {
            Ok(resp) => resp,
            Err(err) => {
                eprintln!("[!] Erro de rede no modelo {model}: {err}. Tentando próximo...");
                last_error = err.into();
                continue;
            }
        };

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            eprintln!("[!] Modelo {model} retornou status {status}: {body}. Tentando próximo...");
            last_error = anyhow::anyhow!("HTTP {status} em {model}: {body}");
            continue;
        }

        let gemini_resp: GeminiResponse = response
            .json()
            .await
            .with_context(|| format!("Falha ao decodificar resposta HTTP do modelo {model}"))?;

        let raw_text = gemini_resp
            .candidates
            .and_then(|c| c.into_iter().next())
            .and_then(|c| c.content.parts.into_iter().next())
            .map(|p| p.text)
            .context("Resposta da API do Gemini veio sem conteúdo de texto")?;

        let cleaned = clean_markdown_json(&raw_text);
        let mut listing: ShopeeListing = serde_json::from_str(cleaned).with_context(|| {
            format!("Falha ao fazer parse do JSON retornado por {model}:\n{cleaned}")
        })?;

        listing.model_used = model.to_string();
        return Ok(listing);
    }

    Err(last_error.context("Todos os modelos da cadeia de fallback falharam"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shopee_listing_bilingual_aliases() {
        let json_en = r#"{
            "title": "Miniatura 3D Rapi – Nikke - Pintado à Mão",
            "description": "Descrição de teste"
        }"#;
        let parsed_en: ShopeeListing = serde_json::from_str(json_en).unwrap();
        assert_eq!(
            parsed_en.titulo,
            "Miniatura 3D Rapi – Nikke - Pintado à Mão"
        );
        assert_eq!(parsed_en.descricao, "Descrição de teste");

        let json_pt = r#"{
            "titulo": "Miniatura 3D BMW M4",
            "descricao": "Descrição PT"
        }"#;
        let parsed_pt: ShopeeListing = serde_json::from_str(json_pt).unwrap();
        assert_eq!(parsed_pt.titulo, "Miniatura 3D BMW M4");
    }

    #[test]
    fn test_listing_request_empty_validation() {
        let empty_req = ListingRequest {
            image: None,
            url: None,
            info: None,
        };
        assert!(empty_req.validate().is_err());
    }
}
