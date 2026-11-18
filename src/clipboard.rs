use anyhow::{Context, Result};
use arboard::Clipboard;

pub struct ClipboardManager {
    inner: Clipboard,
}

impl ClipboardManager {
    pub fn new() -> Result<Self> {
        let inner = Clipboard::new()
            .context("falha ao inicializar o acesso a area de transferencia do sistema")?;
        Ok(Self { inner })
    }

    pub fn copy(&mut self, text: &str) -> Result<()> {
        self.inner
            .set_text(text)
            .context("falha ao copiar texto para area de transferencia")
    }
}
