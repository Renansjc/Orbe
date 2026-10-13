use std::{env, io, process::ExitCode};

use mt01_matter_probe::{
    aliases::Aliases,
    backend,
    domain::{CommissioningCode, LightDevice},
    state,
};

#[tokio::main]
async fn main() -> ExitCode {
    match run(env::args().skip(1).collect()).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("Erro: {message}");
            ExitCode::FAILURE
        }
    }
}

async fn run(arguments: Vec<String>) -> Result<(), String> {
    match arguments.first().map(String::as_str) {
        Some("status") => status(),
        Some("commission") => commission().await,
        Some("discover") => discover(),
        Some("lights") => lights(arguments.get(1).map(String::as_str) == Some("--raw")).await,
        Some("names") => {
            for line in backend::inspect_names().await.map_err(|e| e.to_string())? {
                println!("{line}");
            }
            Ok(())
        }
        Some("identify") => identify(arguments.get(1).map(String::as_str) == Some("--all")).await,
        Some("name") => name(&arguments[1..]),
        Some("mark") => mark(&arguments[1..]),
        Some("link") => link(&arguments[1..]),
        Some("set") => set(&arguments[1..]).await,
        Some("bridges") => bridges().await,
        Some("inspect") => inspect().await,
        _ => {
            println!("MT-01 Matter Probe\n  status\n  discover\n  commission\n  bridges\n  inspect\n  lights [--raw]\n  identify [--all]\n  name <endpoint> <nome> [comodo]   (luz ou painel)\n  mark <endpoint> acao|luz\n  link <a> <b>   (mesma luz em paralelo)\n  set <endpoint> on|off");
            Ok(())
        }
    }
}

async fn lights(raw: bool) -> Result<(), String> {
    if !raw {
        let aliases = Aliases::load().map_err(|error| error.to_string())?;
        let lights = named_lights().await?;
        let mut last_parent = None;
        for light in &lights {
            if light.parent_endpoint != last_parent {
                last_parent = light.parent_endpoint;
                match light.parent_endpoint {
                    Some(parent) => println!("Painel {parent}{}:", panel_label(&aliases, parent)),
                    None => println!("Sem painel:"),
                }
            }
            // Em paralelo, mostra só o menor endpoint do par.
            if light.twin_endpoint.is_some_and(|twin| twin < light.endpoint_id) {
                continue;
            }
            let diverged = light.twin_endpoint.is_some_and(|twin| {
                lights.iter().any(|other| other.endpoint_id == twin && other.on != light.on)
            });
            println!(
                "  {}{}",
                light.summary(),
                if diverged { "  (estados divergentes!)" } else { "" }
            );
        }
        println!("{} luzes (use `lights --raw` para os rótulos brutos).", lights.len());
        return Ok(());
    }
    for report in backend::inspect_lights().await.map_err(|error| error.to_string())? {
        println!("{report}");
    }
    Ok(())
}

fn panel_label(aliases: &Aliases, panel: u16) -> String {
    match aliases.get(panel) {
        Some(alias) if !alias.name.is_empty() => {
            let room = if alias.room.is_empty() { String::new() } else { format!(", {}", alias.room) };
            format!(" — {}{room}", alias.name)
        }
        _ => String::new(),
    }
}

fn parse_endpoint(text: &str) -> Result<u16, String> {
    text.parse::<u16>().map_err(|_| "endpoint invalido".to_string())
}

fn name(args: &[String]) -> Result<(), String> {
    let [endpoint, name, rest @ ..] = args else {
        return Err("use: name <endpoint> <nome> [comodo]".into());
    };
    let endpoint = parse_endpoint(endpoint)?;
    let mut aliases = Aliases::load().map_err(|error| error.to_string())?;
    aliases.set(endpoint, name, rest.first().map_or("", String::as_str));
    aliases.save().map_err(|error| error.to_string())?;
    println!("Salvo: [{endpoint}] {name}");
    Ok(())
}

fn mark(args: &[String]) -> Result<(), String> {
    let [endpoint, kind] = args else {
        return Err("use: mark <endpoint> acao|luz".into());
    };
    let endpoint = parse_endpoint(endpoint)?;
    let action = match kind.as_str() {
        "acao" => true,
        "luz" => false,
        _ => return Err("use: mark <endpoint> acao|luz".into()),
    };
    let mut aliases = Aliases::load().map_err(|error| error.to_string())?;
    aliases.set_action(endpoint, action);
    aliases.save().map_err(|error| error.to_string())?;
    println!("[{endpoint}] agora e {}.", if action { "acao (rotina)" } else { "luz" });
    Ok(())
}

fn link(args: &[String]) -> Result<(), String> {
    let [a, b] = args else {
        return Err("use: link <a> <b>".into());
    };
    let (a, b) = (parse_endpoint(a)?, parse_endpoint(b)?);
    if a == b {
        return Err("os endpoints precisam ser diferentes".into());
    }
    let mut aliases = Aliases::load().map_err(|error| error.to_string())?;
    aliases.link(a, b);
    aliases.save().map_err(|error| error.to_string())?;
    println!("[{a}] e [{b}] agora sao a mesma luz (paralelo).");
    Ok(())
}

async fn named_lights() -> Result<Vec<LightDevice>, String> {
    let aliases = Aliases::load().map_err(|error| error.to_string())?;
    let mut lights = backend::list_lights().await.map_err(|error| error.to_string())?;
    for light in &mut lights {
        light.apply_alias(&aliases);
    }
    lights.sort_by_key(|light| (light.parent_endpoint, light.endpoint_id));
    Ok(lights)
}

fn prompt(question: &str) -> Result<String, String> {
    println!("{question}");
    let mut input = String::new();
    io::stdin().read_line(&mut input).map_err(|error| error.to_string())?;
    Ok(input.trim().to_owned())
}

/// Identificação guiada: alterna cada botão, pergunta nome e cômodo e restaura
/// o estado original. ALTERA HARDWARE; cada botão exige confirmação.
async fn identify(all: bool) -> Result<(), String> {
    let mut aliases = Aliases::load().map_err(|error| error.to_string())?;
    let lights = named_lights().await?;
    let pending: Vec<_> = lights
        .iter()
        .filter(|light| all || aliases.get(light.endpoint_id).is_none_or(|a| a.name.is_empty()))
        .collect();
    if pending.is_empty() {
        println!("Todas as luzes ja tem nome. Use `identify --all` para refazer.");
        return Ok(());
    }
    println!(
        "{} luzes para identificar. Cada uma sera alternada por instantes e restaurada.",
        pending.len()
    );
    println!("Respostas: 's' alterna a luz, Enter pula, 'q' sai (o que ja foi salvo fica).");

    for light in pending {
        let panel = light
            .parent_endpoint
            .map_or("sem painel".to_owned(), |parent| format!("painel {parent}"));
        let answer = prompt(&format!(
            "
[{}] ({panel}) esta {}. Alternar para identificar? (s/Enter/q)",
            light.endpoint_id,
            if light.on { "ligada" } else { "desligada" }
        ))?;
        match answer.to_lowercase().as_str() {
            "q" => break,
            "s" => {}
            _ => continue,
        }

        let toggled = backend::set_light(light.endpoint_id, !light.on)
            .await
            .map_err(|error| error.to_string())?;
        println!("Agora: {}", toggled.summary());
        let name = prompt("Qual luz mudou? Nome (Enter para nao salvar):");
        // Sempre restaura o estado original, mesmo se a leitura da resposta falhar.
        let restored = backend::set_light(light.endpoint_id, light.on).await;
        match &restored {
            Ok(_) => println!("Estado original restaurado."),
            Err(error) => eprintln!("ATENCAO: nao consegui restaurar a luz {}: {error}", light.endpoint_id),
        }
        let name = name?;
        if name.is_empty() {
            continue;
        }
        let room = prompt("Comodo (Enter para nenhum):")?;
        aliases.set(light.endpoint_id, &name, &room);
        aliases.save().map_err(|error| error.to_string())?;
        println!("Salvo: [{}] {name}", light.endpoint_id);
    }
    Ok(())
}

async fn set(args: &[String]) -> Result<(), String> {
    let (endpoint, on) = match args {
        [endpoint, state] => (
            endpoint.parse::<u16>().map_err(|_| "endpoint invalido".to_string())?,
            match state.as_str() {
                "on" => true,
                "off" => false,
                _ => return Err("use: set <endpoint> on|off".into()),
            },
        ),
        _ => return Err("use: set <endpoint> on|off".into()),
    };

    let current = named_lights()
        .await?
        .into_iter()
        .find(|light| light.endpoint_id == endpoint)
        .ok_or_else(|| format!("endpoint {endpoint} nao e uma luz On/Off"))?;
    println!("Atual: {}", current.summary());
    println!(
        "Vai {} o endpoint {endpoint} (node {}). Isso altera uma luz real.",
        if on { "LIGAR" } else { "DESLIGAR" },
        current.node_id
    );
    println!("Digite 's' e Enter para confirmar:");
    let mut input = String::new();
    io::stdin().read_line(&mut input).map_err(|error| error.to_string())?;
    if !input.trim().eq_ignore_ascii_case("s") {
        println!("Cancelado. Nada foi alterado.");
        return Ok(());
    }

    let mut after = backend::set_light(endpoint, on).await.map_err(|error| error.to_string())?;
    after.apply_alias(&Aliases::load().map_err(|error| error.to_string())?);
    println!("Depois: {}", after.summary());
    if after.on != on {
        return Err("o comando foi aceito, mas o estado lido nao mudou".into());
    }
    Ok(())
}

fn discover() -> Result<(), String> {
    println!("Procurando janelas Matter ativas por 20 segundos...");
    let services = backend::discover_commissionable(std::time::Duration::from_secs(20))
        .map_err(|error| error.to_string())?;
    if services.is_empty() {
        println!("Nenhuma janela Matter encontrada.");
    } else {
        for service in services { println!("- {service}"); }
    }
    Ok(())
}

fn status() -> Result<(), String> {
    let local_state = state::load().map_err(|error| error.to_string())?;
    println!("Dados locais: {}", state::app_data_dir().display());
    println!("PAA roots: {}", state::paa_roots_dir().display());
    println!("CD roots: {}", state::cd_roots_dir().display());
    println!(
        "Pareamento Matter: {}",
        if local_state.pending_commissioning { "aguardando conclusao" } else { "ainda nao iniciado" }
    );
    Ok(())
}

async fn commission() -> Result<(), String> {
    println!("Cole o codigo Matter e pressione Enter.");
    let mut input = String::new();
    io::stdin().read_line(&mut input).map_err(|error| error.to_string())?;
    let code = CommissioningCode::parse(&input)?;
    println!("Pareando {} localmente...", code.redacted());
    let bridge = backend::commission(code.as_str()).await.map_err(|error| error.to_string())?;
    state::save(&state::LocalState { pending_commissioning: false })
        .map_err(|error| error.to_string())?;
    println!("Pareado: {} (node {}).", bridge.name, bridge.node_id);
    Ok(())
}

async fn bridges() -> Result<(), String> {
    for bridge in backend::bridges().await.map_err(|error| error.to_string())? {
        println!("- {} (node {})", bridge.name, bridge.node_id);
    }
    Ok(())
}

async fn inspect() -> Result<(), String> {
    for report in backend::inspect_bridges().await.map_err(|error| error.to_string())? {
        println!("{report}");
    }
    Ok(())
}
