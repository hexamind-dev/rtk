# Guia de instalação do RTK — Para assistentes de código com IA

[English](INSTALL.md)

## Aviso de homônimos

**Existem DOIS projetos completamente diferentes chamados "rtk":**

1. **Rust Token Killer** (este projeto) — otimizador de tokens para LLM
   - Repositório: `hexamind-dev/rtk`
   - Comando `rtk gain` para estatísticas de economia de tokens

2. **Rust Type Kit** (reachingforthejack/rtk) — **OUTRO PROJETO**
   - Ferramenta de consulta ao código Rust e gerador de tipos
   - **NÃO instale** se você quer otimização de tokens para LLM

## Verificação pré-instalação (obrigatória)

**Assistentes de IA devem sempre verificar se o RTK já está instalado antes de tentar instalar.**

```bash
# Verificar se o RTK está instalado
rtk --version

# CRÍTICO: confirmar que é o Token Killer (não o Type Kit)
rtk gain    # Deve mostrar estatísticas de economia, NÃO "command not found"

# Caminho de instalação
which rtk
```

Se `rtk gain` funcionar, o RTK **correto** está instalado. **Não reinstale**. Pule para "Inicialização do projeto".

Se `rtk gain` falhar mas `rtk --version` funcionar, você tem o RTK **errado** (Type Kit). Desinstale e reinstale o correto (veja abaixo).

## Instalação (somente se o RTK não existir ou estiver errado)

### Passo 0: desinstalar o RTK errado (se necessário)

Se você instalou o Rust Type Kit por engano:

```bash
cargo uninstall rtk
```

### Instalação rápida (Linux/macOS)

```bash
curl -fsSL https://raw.githubusercontent.com/hexamind-dev/rtk/refs/heads/master/install.sh | sh
```

Após instalar, **confirme que é o rtk certo**:

```bash
rtk gain  # Deve mostrar estatísticas de economia (não "command not found")
```

### Alternativa: instalação manual

```bash
# A partir de hexamind-dev/rtk (NÃO o reachingforthejack!)
cargo install --git https://github.com/hexamind-dev/rtk

# OU (se publicado e correto no crates.io)
cargo install rtk

# SEMPRE verifique após instalar
rtk gain  # DEVE mostrar economia de tokens, não "command not found"
```

**Aviso:** `cargo install rtk` a partir do crates.io pode instalar o pacote errado. Sempre confira com `rtk gain`.

## Inicialização do projeto

### Qual modo escolher?

```
  Você quer o RTK ativo em TODOS os projetos do Claude Code?
  │
  ├─ SIM → rtk init -g              (recomendado)
  │         Hook + RTK.md (~10 tokens no contexto)
  │         Comandos reescritos automaticamente
  │
  ├─ SIM, mínimo → rtk init -g --hook-only
  │         Só o hook, nada no CLAUDE.md
  │         Zero tokens extras no contexto
  │
  └─ NÃO, projeto único → rtk init
            Só CLAUDE.md local
            Sem hook, sem efeito global
```

### Recomendado: configuração global com hook primeiro

**Ideal para: todos os projetos, uso automático do RTK**

```bash
rtk init -g
# → Instala o hook em ~/.claude/hooks/rtk-rewrite.sh
# → Cria ~/.claude/RTK.md (10 linhas, só meta-comandos)
# → Adiciona referência @RTK.md em ~/.claude/CLAUDE.md
# → Pergunta: "Aplicar patch em settings.json? [y/N]"
# → Se sim: aplica patch e cria backup (~/.claude/settings.json.bak)

# Alternativas automatizadas:
rtk init -g --auto-patch    # Patch sem perguntar
rtk init -g --no-patch      # Só imprime instruções manuais

# Verificar instalação
rtk init --show  # Confere se o hook está instalado e executável
```

**Economia de tokens:** ~99,5% de redução (2000 tokens → 10 tokens no contexto)

**O que é settings.json?**  
Registro de hooks do Claude Code. O RTK adiciona um hook PreToolUse que reescreve comandos de forma transparente. Sem isso, o Claude não invoca o hook automaticamente.

```
  Claude Code          settings.json        rtk-rewrite.sh        binário RTK
       │                    │                     │                    │
       │  "git status"      │                     │                    │
       │ ──────────────────►│                     │                    │
       │                    │  gatilho PreToolUse │                    │
       │                    │ ───────────────────►│                    │
       │                    │                     │  reescreve comando │
       │                    │                     │  → rtk git status  │
       │                    │◄────────────────────│                    │
       │                    │  comando atualizado │                    │
       │  executa: rtk git status                                      │
       │ ─────────────────────────────────────────────────────────────►│
       │                                                               │  filtro
       │  "3 modified, 1 untracked ✓"                                  │
       │◄──────────────────────────────────────────────────────────────│
```

**Backup:** o RTK faz backup do `settings.json` antes de alterar. Restauração:

```bash
cp ~/.claude/settings.json.bak ~/.claude/settings.json
```

### Alternativa: configuração só no projeto

**Ideal para: um único projeto, sem hook**

```bash
cd /caminho/do/seu/projeto
rtk init  # Cria ./CLAUDE.md com instruções completas do RTK (137 linhas)
```

**Economia:** instruções carregadas só nesse projeto.

### Atualização de versão anterior

#### Do bloco antigo de 137 linhas no CLAUDE.md (antes da 0.22)

```bash
rtk init -g  # Migra automaticamente para o modo hook-first
# → Remove o bloco antigo de 137 linhas
# → Instala hook + RTK.md
# → Adiciona referência @RTK.md
```

#### Do hook antigo com lógica inline (antes da 0.24) — mudança de compatibilidade

A RTK 0.24.0 substituiu o hook com detecção inline (~200 linhas) por um **delegador fino** que chama `rtk rewrite`. A lógica de reescrita está no binário.

```bash
# Atualizar o hook para o delegador fino
rtk init --global

# Verificar o novo hook
rtk init --show
# Deve mostrar: Hook: ... (thin delegator, up to date)
```

## Fluxos comuns

### Primeiro uso (recomendado)

```bash
# 1. Instalar RTK
cargo install --git https://github.com/hexamind-dev/rtk
rtk gain  # Verificar (deve mostrar estatísticas)

# 2. Configurar com prompts
rtk init -g
# → Responda 'y' quando pedir patch ao settings.json
# → Backup criado automaticamente

# 3. Reiniciar o Claude Code
# 4. Testar: git status (deve passar pelo rtk)
```

### CI/CD ou automação

```bash
# Configuração não interativa (sem prompts)
rtk init -g --auto-patch

# Verificar em scripts
rtk init --show | grep "Hook:"
```

### Uso conservador (controle manual)

```bash
# Instruções manuais sem aplicar patch
rtk init -g --no-patch

# Revise o JSON impresso
# Edite ~/.claude/settings.json manualmente
# Reinicie o Claude Code
```

### Teste temporário

```bash
rtk init -g --auto-patch

# Depois: remover tudo
rtk init -g --uninstall

# Restaurar backup se precisar
cp ~/.claude/settings.json.bak ~/.claude/settings.json
```

## Verificação da instalação

```bash
# Teste básico
rtk ls .

# Teste com git
rtk git status

# Teste com pnpm (opcional no fork)
rtk pnpm list

# Teste com Vitest (branch feat/vitest-support, se existir)
rtk vitest run
```

## Desinstalação

### Remoção completa (instalações globais)

```bash
rtk init -g --uninstall

# O que é removido:
#   - Hook: ~/.claude/hooks/rtk-rewrite.sh
#   - Contexto: ~/.claude/RTK.md
#   - Referência: linha @RTK.md em ~/.claude/CLAUDE.md
#   - Registro: entrada do hook RTK em settings.json

# Reinicie o Claude Code após desinstalar
```

**Projetos locais:** remova manualmente o bloco do RTK em `./CLAUDE.md`.

### Remoção do binário

```bash
# Se instalou via cargo
cargo uninstall rtk

# Se instalou via gerenciador de pacotes
brew uninstall rtk          # macOS Homebrew
sudo apt remove rtk         # Debian/Ubuntu
sudo dnf remove rtk         # Fedora/RHEL
```

### Restaurar backup

```bash
cp ~/.claude/settings.json.bak ~/.claude/settings.json
```

## Comandos essenciais

### Arquivos

```bash
rtk ls .              # Árvore compacta
rtk read file.rs      # Leitura otimizada
rtk grep "pattern" .  # Busca agrupada
```

### Git

```bash
rtk git status        # Status compacto
rtk git log -n 10     # Logs condensados
rtk git diff          # Diff otimizado
rtk git add .         # → "ok ✓"
rtk git commit -m "msg"  # → "ok ✓ abc1234"
rtk git push          # → "ok ✓ main"
```

### Pnpm (fork)

```bash
rtk pnpm list         # Árvore de dependências (-70% tokens)
rtk pnpm outdated     # Atualizações disponíveis (-80–90%)
rtk pnpm install pkg  # Instalação silenciosa
```

### Testes

```bash
rtk test cargo test   # Só falhas (-90%)
rtk vitest run        # Saída Vitest filtrada (-99,6%)
```

### Estatísticas

```bash
rtk gain              # Economia de tokens
rtk gain --graph      # Com gráfico ASCII
rtk gain --history    # Com histórico de comandos
```

## Economia de tokens validada

### Projeto T3 em produção

| Operação | Padrão | RTK | Redução |
|----------|--------|-----|---------|
| `vitest run` | 102.199 caracteres | 377 caracteres | **-99,6%** |
| `git status` | 529 caracteres | 217 caracteres | **-59%** |
| `pnpm list` | ~8.000 tokens | ~2.400 | **-70%** |
| `pnpm outdated` | ~12.000 tokens | ~1.200–2.400 | **-80–90%** |

### Sessão típica no Claude Code (30 min)

- **Sem RTK:** ~150.000 tokens
- **Com RTK:** ~45.000 tokens
- **Economia:** ~**70%**

## Solução de problemas

### Comando `rtk` não encontrado após instalar

```bash
echo $PATH | grep -o '[^:]*\.cargo[^:]*'

export PATH="$HOME/.cargo/bin:$PATH"   # ~/.bashrc ou ~/.zshrc

source ~/.bashrc   # ou source ~/.zshrc
```

### Comando RTK indisponível (ex.: vitest)

```bash
cd /caminho/do/rtk
git branch

git checkout feat/vitest-support   # se necessário

cargo install --path . --force
```

### Erro de compilação

```bash
rustup update stable

cargo clean
cargo build --release
cargo install --path . --force
```

## Suporte e contribuição

- **Site:** https://www.rtk-ai.app
- **Contato:** contact@rtk-ai.app
- **Problemas comuns:** [docs/TROUBLESHOOTING.md](docs/TROUBLESHOOTING.md)
- **Issues:** https://github.com/hexamind-dev/rtk/issues
- **Pull requests:** https://github.com/hexamind-dev/rtk/pulls

Se você instalou o **rtk errado** (Type Kit), veja [TROUBLESHOOTING.md](docs/TROUBLESHOOTING.md#problem-rtk-gain-command-not-found).

## Checklist para assistentes de IA

Antes de cada sessão:

- [ ] Verificar RTK: `rtk --version`
- [ ] Se não estiver instalado → seguir "Instalação" acima
- [ ] Se o projeto não estiver inicializado → `rtk init`
- [ ] Usar `rtk` para comandos git/pnpm/test/vitest que geram saída muito longa
- [ ] Conferir economia: `rtk gain`

**Regra:** assistentes de IA devem usar `rtk` como proxy para comandos de shell com saída verbosa (git, pnpm, npm, cargo test, vitest, docker, kubectl).
