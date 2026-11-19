# 📦 lootscribe_list

> **Gerador automatizado de títulos e descrições otimizados para SEO na Shopee**, construído em **Rust (Edition 2024)** e alimentado pela **Gemini Multimodal API**.

O `lootscribe_list` analisa imagens locais, URLs de repositórios 3D (MakerWorld, Printables, Cults3D) e metadados técnicos de miniaturas colecionáveis, gerando anúncios completos, padronizados e prontos para publicação — com interface interativa no terminal (**TUI**), processamento em lote (**Batch**) e integração direta com o clipboard do sistema.

---

## ✨ Funcionalidades

- **Interface TUI Reativa (`ratatui` + `crossterm`):**
  - Painel dividido em **Fila de Processamento (35%)** e **Preview do Anúncio (65%)** com contador de caracteres em tempo real.
  - Navegação estilo Vim (`j`/`k`, `Ctrl+d`/`Ctrl+u`), modal de inserção dinâmica (`[a]`/`[i]`), retry instantâneo (`[r]`) e remoção de itens (`[d]`).
  - Persistência incremental automática da sessão em `./output/tui_<timestamp>.json` e `.md`.
- **Análise Multimodal & Entrada Polimórfica:**
  - Aceita qualquer combinação de **Imagem** (`JPG`, `PNG`, `WEBP`), **URL de modelo 3D** e **Metadados** (`-i`, `-u`, `-m`).
  - **Otimização Automática de Imagens:** Arquivos leves (`≤ 350 KB`) são enviados intactos; imagens pesadas são redimensionadas (`≤ 1280px`) e comprimidas em JPEG em thread separada (`spawn_blocking`) via crate `image` antes do envio.
- **Processamento em Lote (`batch`):**
  - Fila produtor-consumidor assíncrona (`tokio::sync::mpsc`) com *jitter* anti-429, deduplicação canônica de URLs (ignorando `#fragment`, `?query` e trailing slash) e exportação incremental em `.json` e `.md`.
- **Fallback Resiliente & JSON Estrito:**
  - Rotação automática entre modelos da família Gemini Flash (`gemini-3.5-flash-lite`, `gemini-3.5-flash`, `gemini-3.8-flash`, `gemini-3.6-flash`, `gemini-flash-latest`) com `responseSchema` travado em `OBJECT` e limpeza de blocos Markdown.
- **Integração com Clipboard Wayland (`wl-copy`):**
  - Atalhos rápidos na TUI e no CLI para copiar apenas o título, apenas a descrição ou ambos formatados.

---

## 🛠️ Stack Tecnológica

- **Linguagem:** [Rust](https://www.rust-lang.org/) (Edition 2024)
- **Runtime & Concorrência:** `tokio` (full), `futures`
- **TUI & CLI:** `ratatui`, `crossterm`, `clap` (derive)
- **HTTP, Serialização & Imagem:** `reqwest`, `serde`, `serde_json`, `base64`, `image`
- **Utilitários & Clipboard:** `anyhow`, `chrono`, `dotenvy`, `rand`, `wl-copy`

---

## 🚀 Instalação e Configuração

### Pré-requisitos

1. **Rust & Cargo** instalados ([rustup.rs](https://rustup.rs/)).
2. Uma chave de API do **Google AI Studio** (`GEMINI_API_KEY`).
3. *(Linux/Wayland)* `wl-clipboard` instalado (`sudo pacman -S wl-clipboard` no Arch Linux).

### Compilação e Instalação

```bash
git clone [https://github.com/vejotaa/lootscribe_list.git](https://github.com/vejotaa/lootscribe_list.git)
cd lootscribe_list
cargo install --path .
```

### Configuração da API Key

Crie um arquivo `.env` na raiz de execução ou exporte a variável no seu shell (`~/.zshrc` ou `~/.bashrc`):

```bash
export GEMINI_API_KEY="sua_chave_aqui"
```

---

## 💻 Modos de Uso

### 1. Interface TUI Interativa (Modo Padrão)

Basta executar o comando sem argumentos (ou passar um arquivo `.txt` inicial com `-f`):

```bash
lootscribe_list
# ou pré-carregando uma lista de URLs/metadados:
lootscribe_list tui -f links.txt
```

**Atalhos na TUI (Modo Normal):**

| Tecla | Ação |
| :--- | :--- |
| `j` / `k` ou `↓` / `↑` | Navega pelos itens da fila |
| `Ctrl+d` / `Ctrl+u` | Rola o preview da descrição para baixo / cima |
| `a` ou `i` | Abre o modal para adicionar URL, metadados e/ou imagem em tempo real |
| `y` | Copia o **Título** do item selecionado para o clipboard |
| `Y` (`Shift+y`) | Copia a **Descrição** do item selecionado para o clipboard |
| `c` | Copia **Título + Descrição** juntos |
| `r` | Reprocessa (*retry*) o item selecionado |
| `d` | Remove o item selecionado da fila |
| `s` | Força o salvamento imediato em `./output/` |
| `q` ou `Esc` | Encerra a TUI restaurando o terminal |

### 2. Modo Único via CLI

Ideal para gerar um anúncio rápido direto pelo terminal sem abrir a TUI:

```bash
lootscribe_list \
  -i "/caminho/para/miniatura.jpg" \
  -u "[https://makerworld.com/en/models/3604553](https://makerworld.com/en/models/3604553)" \
  -m "15cm com base, acabamento com pintura artesanal"
```

### 3. Modo Batch via Terminal (`batch`)

Processa um arquivo `.txt` (formato `url` ou `url | metadados` por linha) ou aceita inserção dinâmica no terminal com intervalo aleatório entre requisições:

```bash
lootscribe_list batch -f links.txt --min-delay 8 --max-delay 20
```

---

## 🗺️ Roadmap de Evolução

- [x] **1. Core Library Desacoplada (`src/lib.rs`)**
  - Isolamento da lógica de negócios (`gemini`, `prompt`, `batch`, `clipboard`) com cobertura de testes unitários (`cargo test`).
- [x] **2. Entrada Polimórfica (Imagem + URL + Metadados)**
  - Validação estrita de URLs, existência de arquivos locais e otimização automática de payload de imagem.
- [x] **3. Batch Processing Assíncrono**
  - Fila produtor-consumidor com deduplicação canônica de URLs, jitter anti-429 e relatórios incrementais em `.json` e `.md`.
- [x] **4. Interface TUI Unificada com `ratatui`**
  - Fila em tempo real, preview com contagem de caracteres, modal de inserção e integração nativa com clipboard.
- [ ] **5. Próximos Passos (v0.3.0)**
  - Suporte nativo cross-platform ao clipboard via [`arboard`](https://crates.io/crates/arboard) (Windows/X11/Wayland/macOS).
  - Atalho `Ctrl+v` no modal da TUI para colar caminhos/URLs direto do clipboard e navegador de arquivos integrado.

---

## 📄 Licença

Este projeto está sob a licença MIT. Sinta-se livre para estudar, modificar e adaptar para o seu próprio fluxo de e-commerce ou impressão 3D.
