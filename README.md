# Orbe

Aplicativo Windows **local-first** para automação residencial. Começa como controle de luzes na bandeja do sistema e foi pensado para crescer: TV, ar-condicionado, videogames, sensores, tomadas e cenas.

- **Local-first:** fala com o hub por Matter/IP na rede local. Nada passa pela nuvem.
- **Leve e nativo:** núcleo em Rust + interface Tauri 2 (sem navegador completo, sem VM).
- **Seguro:** a atestação Matter nunca é ignorada (raízes PAA/CD de produção) e o código temporário de pareamento nunca é guardado.

## Estrutura

| Pasta | O que é |
|---|---|
| `src/` | Núcleo (`mt01-matter-probe`): domínio, provider Matter, aliases locais, visão para a UI e CLI de diagnóstico. |
| `src-tauri/` | App de bandeja (`orbe-app`): tray, janela do painel e comandos Tauri tipados. |
| `ui/` | Interface em HTML/CSS/JS simples, sem etapa de build. Cada categoria de dispositivo é um módulo. |
| `scripts/` | Instalação das raízes de confiança Matter, diagnóstico de rede e geração do ícone. |

A UI não fala Matter: ela só chama comandos do núcleo. O Matter é o primeiro *provider*; novos conectores (TV, clima, console) entram como módulos novos.

## Dados locais (fora do Git)

Ficam em `%USERPROFILE%\.orbe\` (ou em `ORBE_DATA_DIR`): credenciais da fabric Matter, raízes de confiança e `aliases.tsv` (nomes, cômodos, rotinas e paralelos). **Nada disso deve ser versionado.**

## Primeiros passos

```powershell
# 1. Raízes Matter de produção (uma vez)
powershell -File scripts\install-matter-trust.ps1
# 2. Parear o hub (o código é digitado no terminal, nunca em argumentos)
cargo run -- commission
# 3. Ver as luzes
cargo run -- lights
```

CLI de diagnóstico: `status`, `discover`, `commission`, `bridges`, `inspect`, `lights [--raw]`, `identify [--all]`, `name`, `mark`, `link`, `set <endpoint> on|off`.
Comandos que alteram luzes pedem confirmação no terminal.

## App de bandeja

```powershell
npm install
npx tauri dev      # desenvolvimento
npx tauri build    # instalador
```

Clique esquerdo no ícone abre/oculta o painel; clique direito abre o menu. Fechar ou perder o foco oculta o painel; só **Sair** encerra.

Variáveis de desenvolvimento: `ORBE_SHOW_ON_START` (abre o painel ao iniciar) e `ORBE_KEEP_OPEN` (não oculta ao perder o foco).

## Estado atual

- Matter: On/Off de luzes, leitura de estado, painéis (pais) e paralelos virtuais.
- Ainda não: assinatura de eventos em tempo real, rotinas/cenas, iniciar com o Windows, brilho e cor.
