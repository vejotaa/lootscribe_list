use anyhow::{Context, Result, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use image::ImageFormat;
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use std::io::Cursor;
use std::net::{IpAddr, Ipv4Addr};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::prompt::SYSTEM_INSTRUCTION;

const FALLBACK_MODELS: &[&str] = &[
    "gemini-3.5-flash-lite",
    "gemini-3.6-flash",
    "gemini-3.7-flash",
];

const MAX_IMAGE_DIMENSION: u32 = 1280;

#[derive(Debug, Clone)]
pub struct ListingRequest {
    pub image: Option<PathBuf>,
    pub url: Option<String>,
    pub info: Option<String>,
}

impl ListingRequest {
    /// Garante programaticamente que há ao menos uma referência válida, URL bem-formada e arquivo existente
    pub fn validate(&self) -> Result<()> {
        let has_image = self.image.is_some();
        let has_url = self.url.as_deref().is_some_and(|s| !s.trim().is_empty());
        let has_info = self.info.as_deref().is_some_and(|s| !s.trim().is_empty());

        if !has_image && !has_url && !has_info {
            bail!("Envie ao menos uma referência (imagem, URL ou metadados)!");
        }

        if let Some(img_path) = &self.image {
            if !img_path.exists() || !img_path.is_file() {
                bail!("Arquivo de imagem não encontrado: {}", img_path.display());
            }
            detect_mime_type(img_path)?;
        }

        if let Some(u) = &self.url
            && !u.trim().is_empty()
        {
            let trimmed = u.trim();
            let has_scheme = trimmed.starts_with("http://") || trimmed.starts_with("https://");
            let host_part = trimmed
                .strip_prefix("https://")
                .or_else(|| trimmed.strip_prefix("http://"))
                .unwrap_or("");
            let has_valid_host = !host_part.is_empty()
                && !host_part.contains(char::is_whitespace)
                && host_part.contains('.');

            if !has_scheme || !has_valid_host {
                bail!(
                    "URL inválida ('{trimmed}'). Informe um link completo começando com http:// ou https:// (ex: https://makerworld.com/...)"
                );
            }
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

        // 2. Visual (com redimensionamento automático para evitar payloads gigantes)
        if let Some(image_path) = &self.image {
            let (b64, mime_type) = encode_optimized_image(image_path)?;

            parts.push(json!({
                "inlineData": {
                    "mimeType": mime_type,
                    "data": b64
                }
            }));
        }

        Ok(parts)
    }
}

/// Carrega a imagem, redimensiona se passar de 1280px e comprime em JPEG para upload rápido
fn encode_optimized_image(path: &Path) -> Result<(String, &'static str)> {
    let mime = detect_mime_type(path)?;
    let raw_bytes = std::fs::read(path)
        .with_context(|| format!("Não foi possível ler a imagem: {}", path.display()))?;

    // Se já for leve (< 350 KB), envia os bytes originais sem recompressão
    if raw_bytes.len() <= 350 * 1024 {
        return Ok((STANDARD.encode(&raw_bytes), mime));
    }

    let img = image::load_from_memory(&raw_bytes)
        .with_context(|| format!("Falha ao decodificar imagem: {}", path.display()))?;

    let resized = if img.width() > MAX_IMAGE_DIMENSION || img.height() > MAX_IMAGE_DIMENSION {
        img.resize(
            MAX_IMAGE_DIMENSION,
            MAX_IMAGE_DIMENSION,
            image::imageops::FilterType::Triangle,
        )
    } else {
        img
    };

    let mut buf = Cursor::new(Vec::new());
    resized
        .to_rgb8()
        .write_to(&mut buf, ImageFormat::Jpeg)
        .context("Falha ao comprimir imagem para JPEG")?;

    Ok((STANDARD.encode(buf.into_inner()), "image/jpeg"))
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

fn parse_shopee_listing(cleaned: &str) -> Result<ShopeeListing> {
    if let Ok(listing) = serde_json::from_str::<ShopeeListing>(cleaned) {
        return Ok(listing);
    }

    if let Ok(mut list) = serde_json::from_str::<Vec<ShopeeListing>>(cleaned)
        && let Some(first) = list.drain(..).next()
    {
        return Ok(first);
    }

    serde_json::from_str::<ShopeeListing>(cleaned).map_err(Into::into)
}

pub async fn generate_listing(api_key: &str, req: &ListingRequest) -> Result<ShopeeListing> {
    // Executa leitura/redimensionamento da imagem em thread bloqueante para não travar o runtime Tokio/TUI
    let req_clone = req.clone();
    let parts = tokio::task::spawn_blocking(move || req_clone.build_gemini_parts())
        .await
        .context("Falha na task de preparação de payload")??;

    let client = Client::builder()
        .local_address(IpAddr::V4(Ipv4Addr::UNSPECIFIED))
        .http1_only()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(45))
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
            "responseMimeType": "application/json",
            "responseSchema": {
                "type": "OBJECT",
                "properties": {
                    "titulo": { "type": "STRING" },
                    "descricao": { "type": "STRING"}
                },
                "required": ["titulo", "descricao"]
            }
        }
    });

    let mut last_error = anyhow::anyhow!("Nenhum modelo disponível na lista de fallback");

    // Até 3 rodadas completas pela cadeia de fallback se houver 503/429 temporário
    for attempt in 1..=3 {
        let mut should_retry_backoff = false;

        for &model in FALLBACK_MODELS {
            let endpoint = format!(
                "https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent"
            );

            let response = match client
                .post(&endpoint)
                .header("x-goog-api-key", api_key.trim())
                .json(&payload)
                .send()
                .await
            {
                Ok(resp) => resp,
                Err(err) => {
                    last_error = anyhow::anyhow!("Erro de rede/timeout em {model}: {err:?}");
                    should_retry_backoff = true;
                    continue;
                }
            };

            let status = response.status();
            if !status.is_success() {
                let body = response.text().await.unwrap_or_default();
                if status.as_u16() == 503 || status.as_u16() == 429 {
                    should_retry_backoff = true;
                }
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

            let mut listing = parse_shopee_listing(cleaned).with_context(|| {
                format!("Falha ao fazer parse do JSON retornado por {model}:\n{cleaned}")
            })?;

            listing.model_used = model.to_string();
            return Ok(listing);
        }

        if should_retry_backoff && attempt < 3 {
            tokio::time::sleep(Duration::from_secs(attempt * 3)).await;
        } else {
            break;
        }
    }

    Err(last_error.context("Todos os modelos da cadeia de fallback falharam"))
}
