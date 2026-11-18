use anyhow::{Context, Result, bail};
use std::io::Write;
use std::process::{Command, Stdio};

pub fn copy_to_wayland(text: &str) -> Result<()> {
    let mut child = Command::new("wl-copy")
        .stdin(Stdio::piped())
        .spawn()
        .context("falha ao executar 'wl-copy'")?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(text.as_bytes())
            .context("falha ao enviar texto para o stdin do wl-copy")?;
    }

    let status = child.wait().context("erro ao aguardar termino do wl-copy")?;
    if !status.success() {
        bail!("wl-copy encerrou com codigo de erro: {}", status);
    }

    Ok(())
}
