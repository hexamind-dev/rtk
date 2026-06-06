<p align="center">
  <img src="https://avatars.githubusercontent.com/u/258253854?v=4" alt="RTK - Rust Token Killer" width="500">
</p>

<p align="center">
  <strong>Proxy CLI de alto desempenho que reduz o consumo de tokens de LLM em 60-90%</strong>
</p>

<p align="center">
  <a href="https://github.com/hexamind-dev/rtk/actions"><img src="https://github.com/hexamind-dev/rtk/workflows/Security%20Check/badge.svg" alt="CI"></a>
  <a href="https://github.com/hexamind-dev/rtk/releases"><img src="https://img.shields.io/github/v/release/hexamind-dev/rtk" alt="Release"></a>
  <a href="https://opensource.org/licenses/MIT"><img src="https://img.shields.io/badge/License-MIT-yellow.svg" alt="License: MIT"></a>
  <a href="https://discord.gg/RySmvNF5kF"><img src="https://img.shields.io/discord/1470188214710046894?label=Discord&logo=discord" alt="Discord"></a>
  <a href="https://formulae.brew.sh/formula/rtk"><img src="https://img.shields.io/homebrew/v/rtk" alt="Homebrew"></a>
</p>

<p align="center">
  <a href="https://www.rtk-ai.app">Site</a> &bull;
  <a href="#instalação">Instalar</a> &bull;
  <a href="docs/TROUBLESHOOTING.md">Solução de problemas</a> &bull;
  <a href="ARCHITECTURE.md">Arquitetura</a> &bull;
  <a href="https://discord.gg/RySmvNF5kF">Discord</a>
</p>

<p align="center">
  <a href="README.md">English</a> &bull;
  <a href="README_fr.md">Francais</a> &bull;
  <a href="README_zh.md">中文</a> &bull;
  <a href="README_ja.md">日本語</a> &bull;
  <a href="README_ko.md">한국어</a> &bull;
  <a href="README_es.md">Espanol</a> &bull;
  <a href="README_ptbr.md">Português (Brasil)</a>
</p>

---

O rtk filtra e comprime a saída dos comandos antes de chegar ao contexto do seu LLM. Binário único em Rust, mais de 100 comandos suportados, overhead inferior a 10 ms.

## Economia de tokens (sessão de 30 min no Claude Code)

| Operação | Frequência | Padrão | rtk | Economia |
|-----------|------------|--------|-----|----------|
| `ls` / `tree` | 10x | 2.000 | 400 | -80% |
| `cat` / `read` | 20x | 40.000 | 12.000 | -70% |
| `grep` / `rg` | 8x | 16.000 | 3.200 | -80% |
| `git status` | 10x | 3.000 | 600 | -80% |
| `git diff` | 5x | 10.000 | 2.500 | -75% |
| `git log` | 5x | 2.500 | 500 | -80% |
| `git add/commit/push` | 8x | 1.600 | 120 | -92% |
| `cargo test` / `npm test` | 5x | 25.000 | 2.500 | -90% |
| `ruff check` | 3x | 3.000 | 600 | -80% |
| `pytest` | 4x | 8.000 | 800 | -90% |
| `go test` | 3x | 6.000 | 600 | -90% |
| `docker ps` | 3x | 900 | 180 | -80% |
| **Total** | | **~118.000** | **~23.900** | **-80%** |

> Estimativas com base em projetos médios em TypeScript/Rust. A economia real varia conforme o tamanho do projeto.

## Instalação

### Homebrew (recomendado)

```bash
brew install rtk
```

### Instalação rápida (Linux/macOS)

```bash
curl -fsSL https://raw.githubusercontent.com/hexamind-dev/rtk/refs/heads/master/install.sh | sh
```

> Instala em `~/.local/bin`. Adicione ao PATH se necessário:
> ```bash
> echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.bashrc  # ou ~/.zshrc
> ```

### Cargo

```bash
cargo install --git https://github.com/hexamind-dev/rtk
```

### Binários pré-compilados

Baixe em [releases](https://github.com/hexamind-dev/rtk/releases):
- macOS: `rtk-x86_64-apple-darwin.tar.gz` / `rtk-aarch64-apple-darwin.tar.gz`
- Linux: `rtk-x86_64-unknown-linux-musl.tar.gz` / `rtk-aarch64-unknown-linux-gnu.tar.gz`
- Windows: `rtk-x86_64-pc-windows-msvc.zip`

### Verificar a instalação

```bash
rtk --version   # Deve mostrar "rtk 0.28.2"
rtk gain        # Deve mostrar estatísticas de economia de tokens
```

> **Aviso de homônimos**: existe outro projeto chamado "rtk" (Rust Type Kit) no crates.io. Se `rtk gain` falhar, você instalou o pacote errado. Use `cargo install --git` conforme acima.

## Início rápido

```bash
# 1. Instalar para a sua ferramenta de IA
rtk init -g                     # Claude Code / Copilot (padrão)
rtk init -g --gemini            # Gemini CLI
rtk init -g --codex             # Codex (OpenAI)
rtk init -g --agent cursor      # Cursor
rtk init --agent windsurf       # Windsurf
rtk init --agent cline          # Cline / Roo Code

# 2. Reinicie a ferramenta de IA e teste
git status  # Reescrito automaticamente para rtk git status
```

O hook reescreve de forma transparente comandos Bash (por exemplo, `git status` → `rtk git status`) antes da execução. O Claude não vê a reescrita; só recebe a saída comprimida.

**Importante:** o hook só roda em chamadas à ferramenta Bash. Ferramentas nativas do Claude Code como `Read`, `Grep` e `Glob` não passam pelo hook Bash, então não são reescritas automaticamente. Para obter a saída compacta do RTK nesses fluxos, use comandos de shell (`cat`/`head`/`tail`, `rg`/`grep`, `find`) ou chame `rtk read`, `rtk grep` ou `rtk find` diretamente.

## Como funciona

```
  Sem rtk:                                         Com rtk:

  Claude  --git status-->  shell  -->  git         Claude  --git status-->  RTK  -->  git
    ^                                   |            ^                      |          |
    |        ~2.000 tokens (bruto)      |            |   ~200 tokens        | filtro   |
    +-----------------------------------+            +------- (filtrado) ---+----------+
```

Quatro estratégias por tipo de comando:

1. **Filtragem inteligente** — Remove ruído (comentários, espaços em branco, boilerplate)
2. **Agrupamento** — Agrega itens semelhantes (arquivos por diretório, erros por tipo)
3. **Truncamento** — Mantém contexto relevante, corta redundância
4. **Deduplicação** — Colapsa linhas de log repetidas com contadores

## Comandos

### Arquivos
```bash
rtk ls .                        # Árvore de diretórios otimizada para tokens
rtk read file.rs                # Leitura inteligente de arquivo
rtk read file.rs -l aggressive  # Apenas assinaturas (remove corpos)
rtk smart file.rs               # Resumo heurístico em 2 linhas
rtk find "*.rs" .               # Resultados compactos do find
rtk grep "pattern" .            # Resultados de busca agrupados
rtk diff file1 file2            # Diff condensado
```

### Git
```bash
rtk git status                  # Status compacto
rtk git log -n 10               # Commits em uma linha
rtk git diff                    # Diff condensado
rtk git add                     # -> "ok"
rtk git commit -m "msg"         # -> "ok abc1234"
rtk git push                    # -> "ok main"
rtk git pull                    # -> "ok 3 files +10 -2"
```

### GitHub CLI
```bash
rtk gh pr list                  # Lista compacta de PRs
rtk gh pr view 42               # Detalhes do PR + checks
rtk gh issue list               # Lista compacta de issues
rtk gh run list                 # Status dos workflow runs
```

### Executores de teste
```bash
rtk test cargo test             # Só falhas (-90%)
rtk err npm run build           # Só erros e avisos
rtk vitest run                  # Vitest compacto (só falhas)
rtk playwright test             # E2E (só falhas)
rtk pytest                      # Testes Python (-90%)
rtk go test                     # Testes Go (NDJSON, -90%)
rtk cargo test                  # Testes Cargo (-90%)
rtk rake test                   # Ruby minitest (-90%)
rtk rspec                       # RSpec (JSON, -60%+)
```

### Build e lint
```bash
rtk lint                        # ESLint agrupado por regra/arquivo
rtk lint biome                  # Outros linters suportados
rtk tsc                         # Erros TypeScript agrupados por arquivo
rtk next build                  # Build Next.js compacto
rtk prettier --check .          # Arquivos que precisam de formatação
rtk cargo build                 # Cargo build (-80%)
rtk cargo clippy                # Cargo clippy (-80%)
rtk ruff check                  # Lint Python (JSON, -80%)
rtk golangci-lint run           # Lint Go (JSON, -85%)
rtk rubocop                     # Lint Ruby (JSON, -60%+)
```

### Gerenciadores de pacotes
```bash
rtk pnpm list                   # Árvore de dependências compacta
rtk pip list                    # Pacotes Python (detecta uv automaticamente)
rtk pip outdated                # Pacotes desatualizados
rtk bundle install              # Gems Ruby (remove linhas "Using")
rtk prisma generate             # Geração de schema (sem arte ASCII)
```

### Contêineres
```bash
rtk docker ps                   # Lista compacta de contêineres
rtk docker images               # Lista compacta de imagens
rtk docker logs <container>     # Logs deduplicados
rtk docker compose ps           # Serviços do Compose
rtk kubectl pods                # Lista compacta de pods
rtk kubectl logs <pod>          # Logs deduplicados
rtk kubectl services            # Lista compacta de serviços
```

### Dados e análise
```bash
rtk json config.json            # Estrutura sem valores
rtk deps                        # Resumo de dependências
rtk env -f AWS                  # Variáveis de ambiente filtradas
rtk log app.log                 # Logs deduplicados
rtk curl <url>                  # Detecta JSON automaticamente + schema
rtk wget <url>                  # Download, remove barras de progresso
rtk summary <comando longo>     # Resumo heurístico
rtk proxy <command>             # Pass-through bruto + rastreamento
```

### Análise de economia de tokens
```bash
rtk gain                        # Estatísticas resumidas
rtk gain --graph                # Gráfico ASCII (últimos 30 dias)
rtk gain --history              # Histórico recente de comandos
rtk gain --daily                # Detalhamento por dia
rtk gain --all --format json    # Export JSON para dashboards

rtk discover                    # Encontrar oportunidades de economia perdidas
rtk discover --all --since 7    # Todos os projetos, últimos 7 dias

rtk session                     # Adoção do RTK nas sessões recentes
```

## Flags globais

```bash
-u, --ultra-compact    # Ícones ASCII, formato inline (economia extra de tokens)
-v, --verbose          # Mais verbosidade (-v, -vv, -vvv)
```

## Exemplos

**Listagem de diretório:**
```
# ls -la (45 linhas, ~800 tokens)        # rtk ls (12 linhas, ~150 tokens)
drwxr-xr-x  15 user staff 480 ...       my-project/
-rw-r--r--   1 user staff 1234 ...       +-- src/ (8 files)
...                                      |   +-- main.rs
                                         +-- Cargo.toml
```

**Operações Git:**
```
# git push (15 linhas, ~200 tokens)       # rtk git push (1 linha, ~10 tokens)
Enumerating objects: 5, done.             ok main
Counting objects: 100% (5/5), done.
Delta compression using up to 8 threads
...
```

**Saída de testes:**
```
# cargo test (200+ linhas em falha)     # rtk test cargo test (~20 linhas)
running 15 tests                          FAILED: 2/15 tests
test utils::test_parse ... ok               test_edge_case: assertion failed
test utils::test_format ... ok              test_overflow: panic at utils.rs:18
...
```

## Hook de reescrita automática

A forma mais eficaz de usar o rtk. O hook intercepta comandos Bash de forma transparente e os reescreve para equivalentes do rtk antes da execução.

**Resultado:** adoção 100% do rtk em todas as conversas e subagentes, sem overhead extra de tokens.

**Escopo:** isso vale apenas para chamadas à ferramenta Bash. Ferramentas nativas do Claude Code como `Read`, `Grep` e `Glob` ignoram o hook; use comandos de shell ou comandos `rtk` explícitos quando quiser a filtragem do RTK.

### Configuração

```bash
rtk init -g                 # Instala hook + RTK.md (recomendado)
rtk init -g --opencode      # Plugin OpenCode (em vez do Claude Code)
rtk init -g --auto-patch    # Não interativo (CI/CD)
rtk init -g --hook-only     # Só o hook, sem RTK.md
rtk init --show             # Verificar instalação
```

Após instalar, **reinicie o Claude Code**.

## Ferramentas de IA suportadas

O RTK suporta 10 ferramentas de codificação com IA. Cada integração reescreve comandos de shell para equivalentes `rtk` com economia de 60-90% de tokens.

| Ferramenta | Instalação | Método |
|------------|------------|--------|
| **Claude Code** | `rtk init -g` | Hook PreToolUse (bash) |
| **GitHub Copilot (VS Code)** | `rtk init -g --copilot` | Hook PreToolUse (`rtk hook copilot`) — reescrita transparente |
| **GitHub Copilot CLI** | `rtk init -g --copilot` | PreToolUse deny-with-suggestion (limitação da CLI) |
| **Cursor** | `rtk init -g --agent cursor` | Hook preToolUse (hooks.json) |
| **Gemini CLI** | `rtk init -g --gemini` | Hook BeforeTool (`rtk hook gemini`) |
| **Codex** | `rtk init -g --codex` | AGENTS.md + instruções RTK.md |
| **Windsurf** | `rtk init --agent windsurf` | .windsurfrules (escopo do projeto) |
| **Cline / Roo Code** | `rtk init --agent cline` | .clinerules (escopo do projeto) |
| **OpenCode** | `rtk init -g --opencode` | Plugin TS (tool.execute.before) |
| **OpenClaw** | `openclaw plugins install ./openclaw` | Plugin TS (before_tool_call) |
| **Mistral Vibe** | Planejado (#800) | Bloqueado no upstream BeforeToolCallback |

### Claude Code (padrão)

```bash
rtk init -g                 # Instala hook + RTK.md
rtk init -g --auto-patch    # Não interativo (CI/CD)
rtk init --show             # Verificar instalação
rtk init -g --uninstall     # Remover
```

### GitHub Copilot (VS Code + CLI)

```bash
rtk init -g --copilot         # Instala hook + instruções
```

Cria `.github/hooks/rtk-rewrite.json` (hook PreToolUse) e `.github/copilot-instructions.md` (consciência no prompt).

O hook (`rtk hook copilot`) detecta o formato automaticamente:
- **Copilot Chat no VS Code**: reescrita transparente via `updatedInput` (igual ao Claude Code)
- **Copilot CLI**: deny-with-suggestion (a CLI ainda não suporta `updatedInput` — veja [copilot-cli#2013](https://github.com/github/copilot-cli/issues/2013))

### Cursor

```bash
rtk init -g --agent cursor
```

Cria `~/.cursor/hooks/rtk-rewrite.sh` e aplica patch em `~/.cursor/hooks.json` com matcher preToolUse. Funciona no editor Cursor e na CLI `cursor-agent`.

### Gemini CLI

```bash
rtk init -g --gemini
rtk init -g --gemini --uninstall
```

Cria `~/.gemini/hooks/rtk-hook-gemini.sh` e aplica patch em `~/.gemini/settings.json` com hook BeforeTool.

### Codex (OpenAI)

```bash
rtk init -g --codex
```

Cria `~/.codex/RTK.md` + `~/.codex/AGENTS.md` com referência `@RTK.md`. O Codex lê isso como instruções globais.

### Windsurf

```bash
rtk init --agent windsurf
```

Cria `.windsurfrules` no projeto atual. O Cascade lê as regras e prefixa comandos com `rtk`.

### Cline / Roo Code

```bash
rtk init --agent cline
```

Cria `.clinerules` no projeto atual. O Cline lê as regras e prefixa comandos com `rtk`.

### OpenCode

```bash
rtk init -g --opencode
```

Cria `~/.config/opencode/plugins/rtk.ts`. Usa o hook `tool.execute.before`.

### OpenClaw

```bash
openclaw plugins install ./openclaw
```

Plugin no diretório `openclaw/`. Usa o hook `before_tool_call`, delega para `rtk rewrite`.

### Mistral Vibe (planejado)

Bloqueado no suporte upstream BeforeToolCallback ([mistral-vibe#531](https://github.com/mistralai/mistral-vibe/issues/531), [PR #533](https://github.com/mistralai/mistral-vibe/pull/533)). Acompanhado em [#800](https://github.com/rtk-ai/rtk/issues/800).

### Comandos reescritos

| Comando bruto | Reescrito para |
|---------------|----------------|
| `git status/diff/log/add/commit/push/pull` | `rtk git ...` |
| `gh pr/issue/run` | `rtk gh ...` |
| `cargo test/build/clippy` | `rtk cargo ...` |
| `cat/head/tail <arquivo>` | `rtk read <arquivo>` |
| `rg/grep <padrão>` | `rtk grep <padrão>` |
| `ls` | `rtk ls` |
| `vitest/jest` | `rtk vitest run` |
| `tsc` | `rtk tsc` |
| `eslint/biome` | `rtk lint` |
| `prettier` | `rtk prettier` |
| `playwright` | `rtk playwright` |
| `prisma` | `rtk prisma` |
| `ruff check/format` | `rtk ruff ...` |
| `pytest` | `rtk pytest` |
| `pip list/install` | `rtk pip ...` |
| `go test/build/vet` | `rtk go ...` |
| `golangci-lint` | `rtk golangci-lint` |
| `rake test` / `rails test` | `rtk rake test` |
| `rspec` / `bundle exec rspec` | `rtk rspec` |
| `rubocop` / `bundle exec rubocop` | `rtk rubocop` |
| `bundle install/update` | `rtk bundle ...` |
| `docker ps/images/logs` | `rtk docker ...` |
| `kubectl get/logs` | `rtk kubectl ...` |
| `curl` | `rtk curl` |
| `pnpm list/outdated` | `rtk pnpm ...` |

Comandos que já usam `rtk`, heredocs (`<<`) e comandos não reconhecidos passam sem alteração.

## Configuração

### Arquivo de configuração

`~/.config/rtk/config.toml` (macOS: `~/Library/Application Support/rtk/config.toml`):

```toml
[tracking]
database_path = "/path/to/custom.db"  # padrão: ~/.local/share/rtk/history.db

[hooks]
exclude_commands = ["curl", "playwright"]  # não reescrever estes

[tee]
enabled = true          # salvar saída bruta em falha (padrão: true)
mode = "failures"       # "failures", "always" ou "never"
max_files = 20          # limite de rotação
```

### Tee: recuperação da saída completa

Quando um comando falha, o RTK salva a saída bruta completa para o LLM ler sem reexecutar:

```
FAILED: 2/15 tests
[full output: ~/.local/share/rtk/tee/1707753600_cargo_test.log]
```

### Desinstalar

```bash
rtk init -g --uninstall     # Remove hook, RTK.md, entrada em settings.json
cargo uninstall rtk          # Remove o binário
brew uninstall rtk           # Se instalado via Homebrew
```

## Documentação

- **[TROUBLESHOOTING.md](docs/TROUBLESHOOTING.md)** — Corrigir problemas comuns
- **[INSTALL_ptbr.md](INSTALL_ptbr.md)** — Guia de instalação detalhado (PT-BR) · [English](INSTALL.md)
- **[ARCHITECTURE.md](ARCHITECTURE.md)** — Arquitetura técnica
- **[SECURITY.md](SECURITY.md)** — Política de segurança e revisão de PRs
- **[AUDIT_GUIDE.md](docs/AUDIT_GUIDE.md)** — Guia de análise de economia de tokens

## Privacidade e telemetria

O RTK **pode** enviar **métricas de uso anônimas e agregadas** no máximo uma vez por dia **somente se você ativar** (`[telemetry] enabled = true` em `~/.config/rtk/config.toml`) e o binário tiver sido compilado com endpoint de telemetria (`RTK_TELEMETRY_URL` na compilação). **O padrão é desligado.**

**O que é coletado** (quando ativado):
- Hash do dispositivo (SHA-256 com salt — salt aleatório por usuário armazenado localmente, não reversível)
- Versão do RTK, SO, arquitetura
- Contagem de comandos (últimas 24 h) e principais nomes de comando (ex.: "git", "cargo" — sem argumentos, sem caminhos de arquivo)
- Percentual de economia de tokens

**O que NÃO é coletado:** código-fonte, caminhos de arquivo, argumentos de comandos, segredos, variáveis de ambiente ou qualquer dado pessoalmente identificável.

**Ativar** (em `~/.config/rtk/config.toml`):
```toml
[telemetry]
enabled = true
```

**Desativar** (mesmo após ativar): `enabled = false` ou `RTK_TELEMETRY_DISABLED=1`.

## Contribuindo

Contribuições são bem-vindas! Abra uma issue ou PR no [GitHub](https://github.com/hexamind-dev/rtk).

Entre na comunidade no [Discord](https://discord.gg/RySmvNF5kF).

## Licença

Licença MIT — veja [LICENSE](LICENSE) para detalhes.
