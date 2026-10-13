use std::{
    sync::Mutex as StdMutex,
    time::{Duration, Instant},
};

use mt01_matter_probe::{
    aliases::Aliases,
    backend::Hub,
    view::{build_snapshot, Snapshot},
};
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, State, WebviewWindow, WindowEvent,
};

/// Um clique no ícone da bandeja tira o foco da janela (ela se oculta) antes
/// de o clique terminar; sem esta folga o painel reabriria na hora.
const REOPEN_GUARD: Duration = Duration::from_millis(300);
const BLUR_GRACE: Duration = Duration::from_millis(400);

struct AppState {
    /// Conexão com o hub, criada sob demanda e descartada ao falhar.
    hub: tokio::sync::Mutex<Option<Hub>>,
    last_hide: StdMutex<Option<Instant>>,
    last_show: StdMutex<Option<Instant>>,
    /// Retângulo do ícone da bandeja em pixels físicos: (x, y, largura, altura).
    tray_rect: StdMutex<Option<(i32, i32, i32, i32)>>,
}

async fn fetch(state: &AppState, command: Option<(u16, bool)>) -> Result<Snapshot, String> {
    let mut guard = state.hub.lock().await;
    if guard.is_none() {
        let hub = Hub::connect()
            .await
            .map_err(|error| format!("Nao foi possivel conectar ao hub: {error}"))?;
        *guard = Some(hub);
    }
    let hub = guard.as_ref().expect("hub criado acima");

    let outcome = async {
        if let Some((endpoint, on)) = command {
            hub.set_light(endpoint, on).await?;
        }
        hub.lights().await
    }
    .await;

    match outcome {
        Ok(lights) => {
            let aliases = Aliases::load().map_err(|error| error.to_string())?;
            Ok(build_snapshot(lights, &aliases))
        }
        Err(error) => {
            *guard = None; // força reconexão na próxima tentativa
            Err(format!("O hub nao respondeu: {error}"))
        }
    }
}

#[tauri::command]
async fn get_lights(state: State<'_, AppState>) -> Result<Snapshot, String> {
    fetch(&state, None).await
}

/// Ação explícita do usuário (clique no cartão): altera uma luz real.
#[tauri::command]
async fn set_light(
    state: State<'_, AppState>,
    endpoint: u16,
    on: bool,
) -> Result<Snapshot, String> {
    let aliases = Aliases::load().map_err(|error| error.to_string())?;
    if aliases.get(endpoint).is_some_and(|alias| alias.action) {
        return Err("Rotinas ainda nao estao habilitadas no Orbe.".into());
    }
    fetch(&state, Some((endpoint, on))).await
}

#[tauri::command]
fn hide_panel(app: AppHandle) {
    hide_window(&app);
}

fn hide_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        if window.is_visible().unwrap_or(false) {
            let _ = window.hide();
            if let Some(state) = app.try_state::<AppState>() {
                *state.last_hide.lock().unwrap() = Some(Instant::now());
            }
        }
    }
}

/// Posiciona o painel acima do ícone da bandeja (ou no canto inferior direito
/// se o ícone for desconhecido), sempre dentro do monitor.
fn place_window(window: &WebviewWindow, tray: Option<(i32, i32, i32, i32)>) {
    let Ok(size) = window.outer_size() else { return };
    let Some(monitor) = window
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten())
    else {
        return;
    };
    let (mx, my) = (monitor.position().x, monitor.position().y);
    let (mw, mh) = (monitor.size().width as i32, monitor.size().height as i32);
    let (w, h) = (size.width as i32, size.height as i32);
    let margin = 8;

    let (mut x, mut y) = match tray {
        Some((tx, ty, tw, th)) => {
            let above = ty - h - margin;
            // Barra de tarefas no topo: abre abaixo do ícone.
            (tx + tw / 2 - w / 2, if above < my { ty + th + margin } else { above })
        }
        None => (mx + mw - w - 12, my + mh - h - 56),
    };
    x = x.clamp(mx + margin, (mx + mw - w - margin).max(mx + margin));
    y = y.clamp(my + margin, (my + mh - h - margin).max(my + margin));
    let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
}

fn show_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else { return };
    let tray = app
        .try_state::<AppState>()
        .and_then(|state| *state.tray_rect.lock().unwrap());
    place_window(&window, tray);
    if let Some(state) = app.try_state::<AppState>() {
        *state.last_show.lock().unwrap() = Some(Instant::now());
    }
    let _ = window.show();
    let _ = window.set_focus();
    let _ = app.emit("panel-shown", ());
}

fn toggle_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else { return };
    if window.is_visible().unwrap_or(false) {
        hide_window(app);
        return;
    }
    let recently_hidden = app
        .try_state::<AppState>()
        .and_then(|state| *state.last_hide.lock().unwrap())
        .is_some_and(|at| at.elapsed() < REOPEN_GUARD);
    if !recently_hidden {
        show_window(app);
    }
}

pub fn run() {
    tauri::Builder::default()
        .manage(AppState {
            hub: tokio::sync::Mutex::new(None),
            last_hide: StdMutex::new(None),
            last_show: StdMutex::new(None),
            tray_rect: StdMutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![get_lights, set_light, hide_panel])
        .setup(|app| {
            let open = MenuItem::with_id(app, "open", "Abrir Orbe", true, None::<&str>)?;
            let refresh = MenuItem::with_id(app, "refresh", "Atualizar", true, None::<&str>)?;
            let separator = PredefinedMenuItem::separator(app)?;
            let quit = MenuItem::with_id(app, "quit", "Sair", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open, &refresh, &separator, &quit])?;

            let mut tray = TrayIconBuilder::with_id("main")
                .tooltip("Orbe")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "open" => show_window(app),
                    "refresh" => {
                        let _ = app.emit("refresh", ());
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        rect,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            let scale = window.scale_factor().unwrap_or(1.0);
                            let position = rect.position.to_physical::<i32>(scale);
                            let size = rect.size.to_physical::<i32>(scale);
                            if let Some(state) = app.try_state::<AppState>() {
                                *state.tray_rect.lock().unwrap() =
                                    Some((position.x, position.y, size.width, size.height));
                            }
                        }
                        toggle_window(app);
                    }
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;

            if let Some(window) = app.get_webview_window("main") {
                let handle = app.handle().clone();
                window.on_window_event(move |event| match event {
                    // Fechar ou perder o foco oculta; só "Sair" encerra o app.
                    WindowEvent::Focused(false) => {
                        // O Windows pode emitir um blur espúrio logo após abrir.
                        let just_shown = handle
                            .try_state::<AppState>()
                            .and_then(|state| *state.last_show.lock().unwrap())
                            .is_some_and(|at| at.elapsed() < BLUR_GRACE);
                        // ORBE_KEEP_OPEN: só para desenvolvimento/inspeção.
                        let keep_open = std::env::var_os("ORBE_KEEP_OPEN").is_some();
                        if !just_shown && !keep_open {
                            hide_window(&handle);
                        }
                    }
                    WindowEvent::CloseRequested { api, .. } => {
                        api.prevent_close();
                        hide_window(&handle);
                    }
                    _ => {}
                });
            }

            // Útil na primeira execução e em desenvolvimento.
            if std::env::var_os("ORBE_SHOW_ON_START").is_some() {
                show_window(app.handle());
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("falha ao iniciar o Orbe");
}
