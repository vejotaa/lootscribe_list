use std::collections::HashSet;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

use crate::batch::canonical_url_key;
use crate::clipboard::ClipboardManager;
use crate::gemini::{ListingRequest, ShopeeListing, generate_listing};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemStatus {
    Pending,
    Processing,
    Completed,
    Failed(String),
}

#[derive(Debug, Clone)]
pub struct TuiQueueItem {
    pub id: usize,
    pub request: ListingRequest,
    pub status: ItemStatus,
    pub listing: Option<ShopeeListing>,
}

impl TuiQueueItem {
    pub fn display_title(&self) -> String {
        if let Some(ref listing) = self.listing {
            return listing.titulo.clone();
        }
        if let Some(ref info) = self.request.info
            && !info.trim().is_empty()
        {
            return info.clone();
        }
        if let Some(ref url) = self.request.url {
            return url.clone();
        }
        if let Some(ref img) = self.request.image {
            return img
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
        }
        format!("Item #{}", self.id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMode {
    Normal,
    InputModal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ModalField {
    #[default]
    Url,
    Info,
    Image,
}

impl ModalField {
    pub fn next(self) -> Self {
        match self {
            Self::Url => Self::Info,
            Self::Info => Self::Image,
            Self::Image => Self::Url,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Self::Url => Self::Image,
            Self::Info => Self::Url,
            Self::Image => Self::Info,
        }
    }
}

#[derive(Debug, Default)]
pub struct ModalForm {
    pub active_field: ModalField,
    pub url: String,
    pub info: String,
    pub image: String,
    pub error_msg: Option<String>,
}

impl ModalForm {
    pub fn active_buffer_mut(&mut self) -> &mut String {
        match self.active_field {
            ModalField::Url => &mut self.url,
            ModalField::Info => &mut self.info,
            ModalField::Image => &mut self.image,
        }
    }

    pub fn clear(&mut self) {
        self.url.clear();
        self.info.clear();
        self.image.clear();
        self.error_msg = None;
        self.active_field = ModalField::Url;
    }
}

#[derive(Debug)]
pub enum WorkerEvent {
    Started(usize),
    Success { id: usize, listing: ShopeeListing },
    Error { id: usize, error: String },
}

pub struct App {
    pub mode: AppMode,
    pub items: Vec<TuiQueueItem>,
    pub selected_index: usize,
    pub preview_scroll: u16,
    pub modal: ModalForm,
    pub status_toast: Option<(String, Instant)>,
    pub spinner_frame: usize,
    pub seen_urls: HashSet<String>,
    pub next_id: usize,
    pub should_quit: bool,
    clipboard: Option<ClipboardManager>,
    worker_tx: mpsc::Sender<TuiQueueItem>,
}

impl App {
    pub fn new(worker_tx: mpsc::Sender<TuiQueueItem>, clipboard: ClipboardManager) -> Self {
        Self::with_optional_clipboard(worker_tx, Some(clipboard))
    }

    pub fn with_optional_clipboard(
        worker_tx: mpsc::Sender<TuiQueueItem>,
        clipboard: Option<ClipboardManager>,
    ) -> Self {
        Self {
            mode: AppMode::Normal,
            items: Vec::new(),
            selected_index: 0,
            preview_scroll: 0,
            modal: ModalForm::default(),
            status_toast: None,
            spinner_frame: 0,
            seen_urls: HashSet::new(),
            next_id: 1,
            should_quit: false,
            clipboard,
            worker_tx,
        }
    }

    pub fn set_toast(&mut self, msg: impl Into<String>) {
        self.status_toast = Some((msg.into(), Instant::now()));
    }

    pub fn on_tick(&mut self) {
        self.spinner_frame = (self.spinner_frame + 1) % 10;
        if let Some((_, created_at)) = &self.status_toast
            && created_at.elapsed() >= Duration::from_secs(3)
        {
            self.status_toast = None;
        }
    }

    pub fn spinner_char(&self) -> &'static str {
        const FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        FRAMES[self.spinner_frame]
    }

    pub fn selected_item(&self) -> Option<&TuiQueueItem> {
        self.items.get(self.selected_index)
    }

    pub fn select_next(&mut self) {
        if !self.items.is_empty() {
            self.selected_index = (self.selected_index + 1).min(self.items.len() - 1);
            self.preview_scroll = 0;
        }
    }

    pub fn select_prev(&mut self) {
        if !self.items.is_empty() {
            self.selected_index = self.selected_index.saturating_sub(1);
            self.preview_scroll = 0;
        }
    }

    pub fn scroll_preview_down(&mut self, lines: u16) {
        self.preview_scroll = self.preview_scroll.saturating_add(lines);
    }

    pub fn scroll_preview_up(&mut self, lines: u16) {
        self.preview_scroll = self.preview_scroll.saturating_sub(lines);
    }

    pub async fn enqueue_request(&mut self, req: ListingRequest) -> Result<(), String> {
        req.validate().map_err(|e| e.to_string())?;

        if let Some(ref url) = req.url {
            let key = canonical_url_key(url);
            if !key.is_empty() && !self.seen_urls.insert(key) {
                return Err("URL já adicionada na fila (duplicada).".to_string());
            }
        }

        let item = TuiQueueItem {
            id: self.next_id,
            request: req,
            status: ItemStatus::Pending,
            listing: None,
        };
        self.next_id += 1;

        self.items.push(item.clone());
        if self.items.len() == 1 {
            self.selected_index = 0;
        }

        self.worker_tx
            .send(item)
            .await
            .map_err(|_| "Canal do worker encerrado".to_string())?;

        Ok(())
    }

    pub async fn retry_selected(&mut self) {
        if let Some(item) = self.items.get_mut(self.selected_index) {
            if item.status == ItemStatus::Processing {
                self.set_toast("Este item já está sendo processado.");
                return;
            }
            item.status = ItemStatus::Pending;
            item.listing = None;
            let cloned = item.clone();
            if self.worker_tx.send(cloned).await.is_ok() {
                self.set_toast("Reenviado para a fila de processamento.");
            }
        }
    }

    pub fn delete_selected(&mut self) {
        if self.items.is_empty() {
            return;
        }
        let removed = self.items.remove(self.selected_index);
        if let Some(ref url) = removed.request.url {
            self.seen_urls.remove(&canonical_url_key(url));
        }
        if self.selected_index >= self.items.len() && !self.items.is_empty() {
            self.selected_index = self.items.len() - 1;
        }
        self.preview_scroll = 0;
        self.set_toast("Item removido da fila.");
    }

    pub fn copy_title(&mut self) {
        if let Some(titulo) = self
            .selected_item()
            .and_then(|i| i.listing.as_ref())
            .map(|l| l.titulo.clone())
        {
            if let Some(ref mut cb) = self.clipboard {
                match cb.copy(&titulo) {
                    Ok(_) => self.set_toast("Título copiado para o clipboard (wl-copy)!"),
                    Err(e) => self.set_toast(format!("Erro ao copiar: {e}")),
                }
            }
        } else {
            self.set_toast("Nenhum título gerado para este item ainda.");
        }
    }

    pub fn copy_description(&mut self) {
        if let Some(desc) = self
            .selected_item()
            .and_then(|i| i.listing.as_ref())
            .map(|l| l.descricao.clone())
        {
            if let Some(ref mut cb) = self.clipboard {
                match cb.copy(&desc) {
                    Ok(_) => self.set_toast("Descrição copiada para o clipboard (wl-copy)!"),
                    Err(e) => self.set_toast(format!("Erro ao copiar: {e}")),
                }
            }
        } else {
            self.set_toast("Nenhuma descrição gerada para este item ainda.");
        }
    }

    pub fn copy_both(&mut self) {
        if let Some(full) = self
            .selected_item()
            .and_then(|i| i.listing.as_ref())
            .map(|l| format!("{}\n\n{}", l.titulo, l.descricao))
        {
            if let Some(ref mut cb) = self.clipboard {
                match cb.copy(&full) {
                    Ok(_) => self.set_toast("Título + Descrição copiados!"),
                    Err(e) => self.set_toast(format!("Erro ao copiar: {e}")),
                }
            }
        }
    }

    pub fn handle_worker_event(&mut self, event: WorkerEvent) {
        match event {
            WorkerEvent::Started(id) => {
                if let Some(item) = self.items.iter_mut().find(|i| i.id == id) {
                    item.status = ItemStatus::Processing;
                }
            }
            WorkerEvent::Success { id, listing } => {
                if let Some(item) = self.items.iter_mut().find(|i| i.id == id) {
                    item.status = ItemStatus::Completed;
                    item.listing = Some(listing);
                }
            }
            WorkerEvent::Error { id, error } => {
                if let Some(item) = self.items.iter_mut().find(|i| i.id == id) {
                    item.status = ItemStatus::Failed(error);
                }
            }
        }
    }

    pub async fn submit_modal(&mut self) {
        let req = ListingRequest {
            url: (!self.modal.url.trim().is_empty()).then(|| self.modal.url.trim().to_string()),
            info: (!self.modal.info.trim().is_empty()).then(|| self.modal.info.trim().to_string()),
            image: (!self.modal.image.trim().is_empty())
                .then(|| PathBuf::from(self.modal.image.trim())),
        };

        match self.enqueue_request(req).await {
            Ok(_) => {
                self.modal.clear();
                self.mode = AppMode::Normal;
                self.selected_index = self.items.len().saturating_sub(1);
                self.set_toast("Item adicionado à fila!");
            }
            Err(err) => {
                self.modal.error_msg = Some(err);
            }
        }
    }
}

pub async fn run_worker(
    api_key: String,
    mut rx: mpsc::Receiver<TuiQueueItem>,
    event_tx: mpsc::Sender<WorkerEvent>,
) {
    while let Some(item) = rx.recv().await {
        let _ = event_tx.send(WorkerEvent::Started(item.id)).await;

        match generate_listing(&api_key, &item.request).await {
            Ok(listing) => {
                let _ = event_tx
                    .send(WorkerEvent::Success {
                        id: item.id,
                        listing,
                    })
                    .await;
            }
            Err(e) => {
                let _ = event_tx
                    .send(WorkerEvent::Error {
                        id: item.id,
                        error: format!("{:#}", e),
                    })
                    .await;
            }
        }

        let jitter_ms = 1200 + (rand::random::<u64>() % 1500);
        tokio::time::sleep(Duration::from_millis(jitter_ms)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_app() -> (App, mpsc::Receiver<TuiQueueItem>) {
        let (tx, rx) = mpsc::channel(32);
        let app = App::with_optional_clipboard(tx, None);
        (app, rx)
    }

    #[tokio::test]
    async fn test_vim_navigation_and_scroll_reset() {
        let (mut app, mut rx) = setup_app();

        for i in 1..=3 {
            let req = ListingRequest {
                url: Some(format!("https://makerworld.com/en/models/{i}")),
                info: None,
                image: None,
            };
            app.enqueue_request(req).await.unwrap();
            let _ = rx.recv().await;
        }

        assert_eq!(app.selected_index, 0);

        // Rola o preview e verifica que navegar reseta o scroll
        app.scroll_preview_down(10);
        assert_eq!(app.preview_scroll, 10);

        app.select_next(); // j
        assert_eq!(app.selected_index, 1);
        assert_eq!(app.preview_scroll, 0);

        app.select_next(); // j
        assert_eq!(app.selected_index, 2);

        // Não deve ultrapassar o último índice
        app.select_next();
        assert_eq!(app.selected_index, 2);

        app.select_prev(); // k
        assert_eq!(app.selected_index, 1);

        // Saturating sub no topo
        app.select_prev();
        app.select_prev();
        assert_eq!(app.selected_index, 0);
    }

    #[tokio::test]
    async fn test_canonical_url_deduplication_and_removal() {
        let (mut app, mut rx) = setup_app();

        let req1 = ListingRequest {
            url: Some("https://MakerWorld.com/en/models/3604553/#profileId-111".to_string()),
            info: None,
            image: None,
        };
        assert!(app.enqueue_request(req1).await.is_ok());
        let _ = rx.recv().await;

        // Mesma URL canônica com query string diferente deve ser bloqueada
        let req_dup = ListingRequest {
            url: Some("https://makerworld.com/en/models/3604553?ref=share".to_string()),
            info: Some("Metadado extra".to_string()),
            image: None,
        };
        let err = app.enqueue_request(req_dup).await.unwrap_err();
        assert!(err.contains("duplicada"));
        assert_eq!(app.items.len(), 1);

        // Após deletar o item da fila, a URL deve ser liberada para reinserção
        app.delete_selected();
        assert!(app.items.is_empty());

        let req_reinsert = ListingRequest {
            url: Some("https://makerworld.com/en/models/3604553".to_string()),
            info: None,
            image: None,
        };
        assert!(app.enqueue_request(req_reinsert).await.is_ok());
    }

    #[tokio::test]
    async fn test_modal_field_cycling_validation_and_submission() {
        let (mut app, mut rx) = setup_app();
        app.mode = AppMode::InputModal;

        // Validação deve falhar com todos os campos vazios
        app.submit_modal().await;
        assert_eq!(app.mode, AppMode::InputModal);
        assert!(app.modal.error_msg.is_some());

        // Testa Tab e Shift+Tab entre os campos
        assert_eq!(app.modal.active_field, ModalField::Url);
        app.modal.active_field = app.modal.active_field.next();
        assert_eq!(app.modal.active_field, ModalField::Info);
        app.modal.active_field = app.modal.active_field.next();
        assert_eq!(app.modal.active_field, ModalField::Image);
        app.modal.active_field = app.modal.active_field.prev();
        assert_eq!(app.modal.active_field, ModalField::Info);

        // Preenche URL inválida
        app.modal.url = "link-invalido".to_string();
        app.submit_modal().await;
        assert_eq!(app.mode, AppMode::InputModal);
        assert!(
            app.modal
                .error_msg
                .as_deref()
                .unwrap()
                .contains("URL inválida")
        );

        // Corrige URL e adiciona metadados
        app.modal.url = "https://makerworld.com/en/models/999".to_string();
        app.modal.info = "22cm, pintura artesanal".to_string();
        app.submit_modal().await;

        // Modal deve fechar, limpar buffers e enfileirar o item
        assert_eq!(app.mode, AppMode::Normal);
        assert!(app.modal.url.is_empty());
        assert!(app.modal.info.is_empty());
        assert!(app.modal.error_msg.is_none());
        assert_eq!(app.items.len(), 1);

        let queued = rx.recv().await.unwrap();
        assert_eq!(
            queued.request.url.as_deref(),
            Some("https://makerworld.com/en/models/999")
        );
        assert_eq!(
            queued.request.info.as_deref(),
            Some("22cm, pintura artesanal")
        );
    }

    #[tokio::test]
    async fn test_worker_event_state_transitions_and_retry() {
        let (mut app, mut rx) = setup_app();

        let req = ListingRequest {
            url: None,
            info: Some("Miniatura Rapi Nikke 1/7".to_string()),
            image: None,
        };
        app.enqueue_request(req).await.unwrap();
        let queued = rx.recv().await.unwrap();
        assert_eq!(app.items[0].status, ItemStatus::Pending);

        // Simula início do processamento
        app.handle_worker_event(WorkerEvent::Started(queued.id));
        assert_eq!(app.items[0].status, ItemStatus::Processing);

        // Simula falha de rede/API
        app.handle_worker_event(WorkerEvent::Error {
            id: queued.id,
            error: "HTTP 429 Too Many Requests".to_string(),
        });
        assert!(matches!(app.items[0].status, ItemStatus::Failed(_)));

        // Aciona retry [r] e verifica reenvio ao canal do worker
        app.retry_selected().await;
        assert_eq!(app.items[0].status, ItemStatus::Pending);
        let retried = rx.recv().await.unwrap();
        assert_eq!(retried.id, queued.id);

        // Simula conclusão com sucesso
        app.handle_worker_event(WorkerEvent::Success {
            id: queued.id,
            listing: ShopeeListing {
                titulo: "Miniatura Rapi Nikke 1/7 Pintada à Mão".to_string(),
                descricao: "Peça colecionável em resina com acabamento artesanal.".to_string(),
                model_used: "gemini-3.5-flash".to_string(),
            },
        });

        assert_eq!(app.items[0].status, ItemStatus::Completed);
        assert_eq!(
            app.items[0].display_title(),
            "Miniatura Rapi Nikke 1/7 Pintada à Mão"
        );
    }
}
