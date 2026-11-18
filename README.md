# 📦 lootscribe-list

> **Gerador automatizado de títulos e descrições otimizados para SEO na Shopee**, construído em **Rust** e alimentado pela **Gemini Multimodal API**.

O `lootscribe-list` analisa imagens (ou links) de miniaturas colecionáveis e peças impressas em 3D, cruza as características visuais com metadados técnicos do produto e gera anúncios completos, padronizados e prontos para publicação — com integração direta à área de transferência do sistema.

---

## ✨ Funcionalidades Atuais

- **Análise Multimodal (Visão + Texto):** Identifica detalhes da escultura, base de apoio, poses, armaduras e acessórios diretamente a partir da imagem enviada (`JPG`, `PNG`, `WEBP`).
- **SEO & Formatação Condicional:** Gera títulos respeitando o limite de 100 caracteres da Shopee e constrói descrições dinâmicas com blocos visuais, adaptando singular/plural de tamanhos e opções de pintura artesanal automaticamente.
- **Fallback Resiliente de Modelos:** Sistema de rotação automática entre modelos da família Gemini Flash (`gemini-3.5-flash-lite`, `gemini-3.5-flash`, `gemini-3.8-flash`, `gemini-3.6-flash`, `gemini-flash-latest`) para contornar instabilidades temporárias (`503`) ou limites de taxa (`429`) sem interromper o fluxo de trabalho.
- **Timeout & Tratamento Estrito de JSON:** Validação estruturada via `serde_json` com limpeza automática de blocos Markdown e tempo limite configurável via `reqwest`.
- **Integração com Clipboard:** Cópia instantânea do título para a área de transferência logo após a geração, com menu interativo no terminal para alternar entre título e descrição.

---

## 🛠️ Stack Tecnológica

- **Linguagem:** [Rust](https://www.rust-lang.org/) (Edition 2021+)
- **Runtime Assíncrono:** `tokio`
- **CLI & Argumentos:** `clap` (derive)
- **Cliente HTTP & Serialização:** `reqwest`, `serde`, `serde_json`, `base64`
- **Tratamento de Erros & Config:** `anyhow`, `dotenvy`
- **Clipboard:** `wl-copy` *(em migração para `arboard` para suporte nativo cross-platform)*

---

## 🚀 Instalação e Uso

### Pré-requisitos

1. **Rust & Cargo** instalados ([rustup.rs](https://rustup.rs/)).
2. Uma chave de API gratuita do **Google AI Studio** (`GEMINI_API_KEY`).
3. *(Linux/Wayland atualmente)* `wl-clipboard` instalado.

### Compilação

```bash
git clone https://github.com/seu-usuario/pp_titledesc.git
cd pp_titledesc
cargo install --path .
```

### Configuração da API Key

Crie um arquivo `.env` no diretório de execução ou exporte a variável globalmente no seu shell (`~/.zshrc` ou `~/.bashrc`):

```bash
export GEMINI_API_KEY="sua_chave_aqui"
```

### Exemplo de Execução

```bash
pp_titledesc \
  -i "/caminho/para/miniatura_batman.jpg" \
  -m "miniatura 3d batman, dc comics, pintura, sem pintura, tamanho: 15cm com base"
```

**Controles interativos após a geração:**
- `[t]` — Copia o **Título** para o clipboard.
- `[d]` — Copia a **Descrição** completa para o clipboard.
- `[q]` — Encerra o programa.

---

## 🗺️ Roadmap de Evolução

O projeto está sendo evoluído de dentro para fora (núcleo desacoplado $\rightarrow$ novas fontes de entrada $\rightarrow$ processamento em lote $\rightarrow$ interface visual) para garantir portabilidade e manutenibilidade:

- [ ] **1. Suporte Multiplataforma (Linux & Windows) + Core Library**
  - Substituir a dependência externa do `wl-copy` pela crate [`arboard`](https://crates.io/crates/arboard) para suporte nativo a Windows, Wayland, X11 e macOS.
  - Isolar a lógica de negócio (`gemini`, `prompt`, `clipboard`) em `src/lib.rs`, desacoplando o motor da camada de apresentação.
- [ ] **2. Entrada Polimórfica (Link do Modelo + Metadados)**
  - Permitir o envio de URLs de repositórios 3D (MakerWorld, Printables, Cults3D) combinadas com metadados como alternativa ao upload de imagem local (`enum InputSource { Image(PathBuf), Url(String) }`).
- [ ] **3. Batch Processing (Processamento em Lote)**
  - Processamento concorrente de múltiplos itens controlado por semáforo (`tokio::sync::Semaphore`) para respeitar os limites de RPM da API gratuita.
  - Exportação consolidada dos anúncios gerados em `.json` ou `.md`.
- [ ] **4. Interface TUI Unificada com `ratatui` + Seletor de Arquivos**
  - Navegador de arquivos integrado no terminal (inspirado no Yazi) para seleção rápida de múltiplas fotos.
  - Painel de fila de processamento em tempo real, preview formatado do anúncio e atalhos rápidos para envio individual ao clipboard.

---

## 📄 Licença

Este projeto está sob a licença MIT. Sinta-se livre para estudar, modificar e adaptar para o seu próprio fluxo de e-commerce ou impressão 3D.
