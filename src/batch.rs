use std::collections::HashSet;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::Local;
use rand::RngExt;
use serde::Serialize;
use tokio::sync::mpsc;

use crate::clipboard::ClipboardManager;
use crate::gemini::{ListingRequest, ShopeeListing, generate_listing};

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum EntryStatus {
    Pending,
    Processing,
    Completed { listing: ShopeeListing },
    Failed { error: String },
}

#[derive(Debug, Clone, Serialize)]
pub struct BatchEntry {
    pub index: usize,
    pub url: Option<String>,
    pub info: Option<String>,
    pub image: Option<PathBuf>,
    pub status: EntryStatus,
}

#[derive(Debug, Serialize)]
struct BatchReport {
    generated_at: String,
    total: usize,
    completed: usize,
    failed: usize,
    pending: usize,
    items: Vec<BatchEntry>,
}

/// Normaliza uma URL para deduplicação consistente (ignora `#fragment`, `?query`, `/` final e case do host)
pub fn canonical_url_key(raw: &str) -> String {
    let trimmed = raw.trim();
    let without_fragment = trimmed.split('#').next().unwrap_or(trimmed);
    let without_query = without_fragment
        .split('?')
        .next()
        .unwrap_or(without_fragment);
    let no_trailing_slash = without_query.trim_end_matches('/');

    if let Some((scheme, rest)) = no_trailing_slash.split_once("://") {
        let mut parts = rest.splitn(2, '/');
        let host = parts.next().unwrap_or("").to_lowercase();
        match parts.next() {
            Some(path) if !path.is_empty() => {
                format!("{}://{}/{}", scheme.to_lowercase(), host, path)
            }
            _ => format!("{}://{}", scheme.to_lowercase(), host),
        }
    } else {
        no_trailing_slash.to_lowercase()
    }
}

/// Lê linhas de um arquivo `.txt` no formato:
/// `https://makerworld.com/...`
/// ou `https://makerworld.com/... | 15cm, pintura artesanal`
pub fn parse_batch_file(path: &Path) -> Result<Vec<ListingRequest>> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Falha ao ler arquivo de lote: {}", path.display()))?;

    let mut requests = Vec::new();
    let mut seen_urls = HashSet::new();

    for (line_num, raw_line) in content.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let (url_part, info_part) = match line.split_once('|') {
            Some((u, i)) => (u.trim(), Some(i.trim().to_string())),
            None => (line, None),
        };

        if url_part.is_empty() {
            continue;
        }

        let req = ListingRequest {
            image: None,
            url: Some(url_part.to_string()),
            info: info_part.filter(|s| !s.is_empty()),
        };
        req.validate()
            .with_context(|| format!("Erro na linha {} do arquivo de lote", line_num + 1))?;

        let key = canonical_url_key(url_part);
        if !seen_urls.insert(key) {
            eprintln!(
                "⚠️  Linha {} ignorada (URL duplicada): {}",
                line_num + 1,
                url_part
            );
            continue;
        }

        requests.push(req);
    }

    Ok(requests)
}

/// Salva snapshots incrementais em `./output/batch_<timestamp>.json` e `.md`
pub fn persist_outputs(entries: &[BatchEntry], json_path: &Path, md_path: &Path) -> Result<()> {
    let completed = entries
        .iter()
        .filter(|e| matches!(e.status, EntryStatus::Completed { .. }))
        .count();
    let failed = entries
        .iter()
        .filter(|e| matches!(e.status, EntryStatus::Failed { .. }))
        .count();
    let pending = entries.len().saturating_sub(completed + failed);

    let report = BatchReport {
        generated_at: Local::now().to_rfc3339(),
        total: entries.len(),
        completed,
        failed,
        pending,
        items: entries.to_vec(),
    };

    let json_str = serde_json::to_string_pretty(&report)?;
    fs::write(json_path, json_str)
        .with_context(|| format!("Erro ao salvar {}", json_path.display()))?;

    let mut md = String::new();
    md.push_str(&format!(
        "# Lote Shopee — {}\n\n- **Total:** {} | **Concluídos:** {} | **Falhas:** {} | **Pendentes:** {}\n\n---\n\n",
        Local::now().format("%d/%m/%Y %H:%M:%S"),
        entries.len(),
        completed,
        failed,
        pending
    ));

    for entry in entries {
        let ref_label = entry
            .url
            .as_deref()
            .or_else(|| entry.info.as_deref())
            .unwrap_or("Sem referência");

        md.push_str(&format!("## #{} — {}\n\n", entry.index + 1, ref_label));
        if let Some(info) = &entry.info {
            md.push_str(&format!("**Metadados:** `{}`\n\n", info));
        }

        match &entry.status {
            EntryStatus::Completed { listing } => {
                md.push_str(&format!("### Título\n```\n{}\n```\n\n", listing.titulo));
                md.push_str(&format!(
                    "### Descrição\n```\n{}\n```\n\n",
                    listing.descricao
                ));
            }
            EntryStatus::Failed { error } => {
                md.push_str(&format!("> ❌ **Falha:** {}\n\n", error));
            }
            EntryStatus::Processing => {
                md.push_str("> ⏳ *Processando...*\n\n");
            }
            EntryStatus::Pending => {
                md.push_str("> 🕒 *Na fila...*\n\n");
            }
        }
        md.push_str("---\n\n");
    }

    fs::write(md_path, md).with_context(|| format!("Erro ao salvar {}", md_path.display()))?;
    Ok(())
}

/// Loop assíncrono do Consumidor (Worker)
async fn run_worker(
    mut rx: mpsc::UnboundedReceiver<(usize, ListingRequest)>,
    state: Arc<Mutex<Vec<BatchEntry>>>,
    api_key: String,
    min_delay: u64,
    max_delay: u64,
    json_path: PathBuf,
    md_path: PathBuf,
) {
    let mut is_first = true;

    while let Some((idx, req)) = rx.recv().await {
        if !is_first && max_delay > 0 {
            let wait_secs = if min_delay >= max_delay {
                min_delay
            } else {
                rand::rng().random_range(min_delay..=max_delay)
            };
            tokio::time::sleep(Duration::from_secs(wait_secs)).await;
        }
        is_first = false;

        {
            let mut list = state.lock().unwrap();
            if let Some(item) = list.get_mut(idx) {
                item.status = EntryStatus::Processing;
            }
        }

        let result = generate_listing(&api_key, &req).await;

        {
            let mut list = state.lock().unwrap();
            if let Some(item) = list.get_mut(idx) {
                item.status = match result {
                    Ok(listing) => EntryStatus::Completed { listing },
                    Err(err) => EntryStatus::Failed {
                        error: format!("{err:#}"),
                    },
                };
            }
            let _ = persist_outputs(&list, &json_path, &md_path);
        }
    }
}

fn enqueue_item(
    req: ListingRequest,
    state: &Arc<Mutex<Vec<BatchEntry>>>,
    tx: &mpsc::UnboundedSender<(usize, ListingRequest)>,
    seen_urls: &mut HashSet<String>,
) -> Result<bool> {
    req.validate()?;

    if let Some(u) = &req.url {
        let key = canonical_url_key(u);
        if !seen_urls.insert(key) {
            println!("⚠️  URL duplicada ignorada neste lote: {u}");
            return Ok(false);
        }
    }

    let idx = {
        let mut list = state.lock().unwrap();
        let idx = list.len();
        list.push(BatchEntry {
            index: idx,
            url: req.url.clone(),
            info: req.info.clone(),
            image: req.image.clone(),
            status: EntryStatus::Pending,
        });
        idx
    };

    tx.send((idx, req))
        .map_err(|_| anyhow::anyhow!("Canal do worker encerrado inesperadamente"))?;
    Ok(true)
}

/// Coleta dinâmica via terminal enquanto o worker já roda em background
fn collect_interactive_inputs(
    state: &Arc<Mutex<Vec<BatchEntry>>>,
    tx: &mpsc::UnboundedSender<(usize, ListingRequest)>,
    seen_urls: &mut HashSet<String>,
) -> Result<()> {
    println!(
        "\n📥 Modo de Inserção Dinâmica (a fila já processa em background enquanto você digita!)"
    );
    println!("   Deixe a URL em branco e aperte [Enter] quando quiser ir para a revisão.\n");

    let stdin = io::stdin();
    loop {
        let count = state.lock().unwrap().len();
        print!("🔗 URL do modelo #{} (ou Enter para revisar): ", count + 1);
        io::stdout().flush()?;

        let mut url_input = String::new();
        stdin.read_line(&mut url_input)?;
        let url_trimmed = url_input.trim();

        if url_trimmed.is_empty() {
            break;
        }

        print!("📝 Metadados extras (ex: 15cm, pintura artesanal) [opcional]: ");
        io::stdout().flush()?;

        let mut info_input = String::new();
        stdin.read_line(&mut info_input)?;
        let info_trimmed = info_input.trim();

        let req = ListingRequest {
            image: None,
            url: Some(url_trimmed.to_string()),
            info: if info_trimmed.is_empty() {
                None
            } else {
                Some(info_trimmed.to_string())
            },
        };

        match enqueue_item(req, state, tx, seen_urls) {
            Ok(true) => {
                let current_len = state.lock().unwrap().len();
                println!(
                    "   ✅ Enfileirado como #{}! Processando em background...\n",
                    current_len
                );
            }
            Ok(false) => println!(),
            Err(err) => println!("   ❌ {err}\n"),
        }
    }

    Ok(())
}

fn clear_screen() {
    print!("\x1B[2J\x1B[1;1H");
    let _ = io::stdout().flush();
}

fn render_current_card(entries: &[BatchEntry], cursor: usize, notice: Option<&str>) {
    clear_screen();

    let total = entries.len();
    let completed = entries
        .iter()
        .filter(|e| matches!(e.status, EntryStatus::Completed { .. }))
        .count();
    let failed = entries
        .iter()
        .filter(|e| matches!(e.status, EntryStatus::Failed { .. }))
        .count();
    let active = total.saturating_sub(completed + failed);

    println!("\n============================================================");
    println!(
        "📦 LOTE: [{}/{}] | ✅ {} prontos | ❌ {} falhas | ⏳ {} na fila",
        cursor + 1,
        total,
        completed,
        failed,
        active
    );
    println!("============================================================");

    let entry = &entries[cursor];
    if let Some(u) = &entry.url {
        println!("🔗 URL:  {u}");
    }
    if let Some(i) = &entry.info {
        println!("📝 Info: {i}");
    }
    println!("------------------------------------------------------------");

    match &entry.status {
        EntryStatus::Pending => {
            println!("🕒 Status: Aguardando na fila (respeitando intervalo anti-429)...");
        }
        EntryStatus::Processing => {
            println!("⏳ Status: Gerando anúncio na API Gemini agora...");
        }
        EntryStatus::Completed { listing } => {
            println!(
                "📌 TÍTULO ({} chars):\n{}\n",
                listing.titulo.chars().count(),
                listing.titulo
            );
            println!("📄 DESCRIÇÃO:\n{}", listing.descricao);
        }
        EntryStatus::Failed { error } => {
            println!("❌ FALHA AO GERAR:\n{error}");
        }
    }

    println!("============================================================");
    println!(
        "[a] Próximo | [r] Anterior | [t] Copiar Título | [d] Copiar Desc | [+] Add URL | [s] Atualizar | [q] Sair"
    );
    if let Some(msg) = notice {
        println!("💬 {msg}");
    }
}

fn print_final_summary(entries: &[BatchEntry], json_path: &Path, md_path: &Path) {
    let mut ok_items = Vec::new();
    let mut err_items = Vec::new();

    for e in entries {
        match &e.status {
            EntryStatus::Completed { listing } => ok_items.push((e, listing)),
            EntryStatus::Failed { error } => err_items.push((e, error.as_str())),
            _ => err_items.push((e, "Não concluído antes do encerramento")),
        }
    }

    println!("\n==================== RELATÓRIO FINAL DO LOTE ====================");
    println!("✅ Sucessos ({}):", ok_items.len());
    for (e, listing) in &ok_items {
        let src = e.url.as_deref().or(e.info.as_deref()).unwrap_or("Item");
        println!("   #{} [{}] -> {}", e.index + 1, src, listing.titulo);
    }

    if !err_items.is_empty() {
        println!("\n❌ Falhas / Pendentes ({}):", err_items.len());
        for (e, err) in &err_items {
            let src = e.url.as_deref().or(e.info.as_deref()).unwrap_or("Item");
            println!("   #{} [{}] -> {}", e.index + 1, src, err);
        }
    }

    println!("\n💾 Arquivos exportados:");
    println!("   • JSON: {}", json_path.display());
    println!("   • MD:   {}", md_path.display());
    println!("=================================================================");
}

pub async fn run_batch_mode(
    api_key: String,
    file: Option<PathBuf>,
    min_delay: u64,
    max_delay: u64,
    clipboard: &mut ClipboardManager,
) -> Result<()> {
    fs::create_dir_all("output").context("Falha ao criar diretório ./output")?;
    let stamp = Local::now().format("%Y%m%d_%H%M%S");
    let json_path = PathBuf::from(format!("output/batch_{stamp}.json"));
    let md_path = PathBuf::from(format!("output/batch_{stamp}.md"));

    let state: Arc<Mutex<Vec<BatchEntry>>> = Arc::new(Mutex::new(Vec::new()));
    let (tx, rx) = mpsc::unbounded_channel::<(usize, ListingRequest)>();
    let mut seen_urls = HashSet::new();

    let worker_state = Arc::clone(&state);
    let worker_json = json_path.clone();
    let worker_md = md_path.clone();
    let worker_handle = tokio::spawn(async move {
        run_worker(
            rx,
            worker_state,
            api_key,
            min_delay,
            max_delay,
            worker_json,
            worker_md,
        )
        .await;
    });

    if let Some(file_path) = file {
        let requests = parse_batch_file(&file_path)?;
        println!(
            "📂 Carregadas {} referências de {}",
            requests.len(),
            file_path.display()
        );
        for req in requests {
            enqueue_item(req, &state, &tx, &mut seen_urls)?;
        }
    } else {
        collect_interactive_inputs(&state, &tx, &mut seen_urls)?;
    }

    if state.lock().unwrap().is_empty() {
        println!("Nenhum item inserido. Encerrando lote.");
        drop(tx);
        let _ = worker_handle.await;
        return Ok(());
    }

    {
        let snapshot = state.lock().unwrap().clone();
        persist_outputs(&snapshot, &json_path, &md_path)?;
    }

    let mut cursor: usize = 0;
    let mut notice: Option<String> = None;
    let stdin = io::stdin();

    loop {
        let snapshot = state.lock().unwrap().clone();
        if cursor >= snapshot.len() {
            cursor = snapshot.len().saturating_sub(1);
        }

        render_current_card(&snapshot, cursor, notice.as_deref());
        notice = None;
        print!("> ");
        io::stdout().flush()?;

        let mut cmd = String::new();
        stdin.read_line(&mut cmd)?;

        match cmd.trim().to_lowercase().as_str() {
            "a" => {
                if cursor + 1 < snapshot.len() {
                    cursor += 1;
                } else {
                    notice = Some("⚠️  Você já está no último modelo da lista.".to_string());
                }
            }
            "r" => {
                if cursor > 0 {
                    cursor -= 1;
                } else {
                    notice = Some("⚠️  Você já está no primeiro modelo da lista.".to_string());
                }
            }
            "t" => match &snapshot[cursor].status {
                EntryStatus::Completed { listing } => {
                    clipboard.copy(&listing.titulo)?;
                    notice = Some(format!(
                        "📋 Título #{} copiado para o clipboard!",
                        cursor + 1
                    ));
                }
                _ => notice = Some("⚠️  Este item ainda não foi concluído ou falhou.".to_string()),
            },
            "d" => match &snapshot[cursor].status {
                EntryStatus::Completed { listing } => {
                    clipboard.copy(&listing.descricao)?;
                    notice = Some(format!(
                        "📋 Descrição #{} copiada para o clipboard!",
                        cursor + 1
                    ));
                }
                _ => notice = Some("⚠️  Este item ainda não foi concluído ou falhou.".to_string()),
            },
            "+" => {
                clear_screen();
                collect_interactive_inputs(&state, &tx, &mut seen_urls)?;
            }
            "s" | "" => {}
            "q" => {
                clear_screen();
                drop(tx);
                let final_snapshot = state.lock().unwrap().clone();
                let _ = persist_outputs(&final_snapshot, &json_path, &md_path);
                print_final_summary(&final_snapshot, &json_path, &md_path);
                break;
            }
            _ => {
                notice = Some("Comando inválido. Use: a, r, t, d, +, s ou q.".to_string());
            }
        }
    }

    worker_handle.abort();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_canonical_url_key_normalization() {
        assert_eq!(
            canonical_url_key("https://MakerWorld.com/en/models/3604553/#profileId-2189745"),
            "https://makerworld.com/en/models/3604553"
        );
        assert_eq!(
            canonical_url_key("https://makerworld.com/en/models/3604553?ref=share"),
            "https://makerworld.com/en/models/3604553"
        );
    }

    #[test]
    fn test_parse_batch_file_dedup_and_metadata() {
        let mut temp_path = std::env::temp_dir();
        temp_path.push("lootscribe_test_links.txt");

        let mut file = fs::File::create(&temp_path).unwrap();
        writeln!(
            file,
            "# Comentário ignorado\n\n\
             https://makerworld.com/model/1 | 15cm, pintura artesanal\n\
             https://makerworld.com/model/2\n\
             https://makerworld.com/model/1/#profileId-999 | duplicada com fragmento deve ser ignorada"
        )
        .unwrap();

        let requests = parse_batch_file(&temp_path).unwrap();
        let _ = fs::remove_file(&temp_path);

        assert_eq!(requests.len(), 2);
        assert_eq!(
            requests[0].url.as_deref(),
            Some("https://makerworld.com/model/1")
        );
        assert_eq!(requests[0].info.as_deref(), Some("15cm, pintura artesanal"));
        assert_eq!(
            requests[1].url.as_deref(),
            Some("https://makerworld.com/model/2")
        );
        assert_eq!(requests[1].info, None);
    }
}
