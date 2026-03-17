use crate::settings::{AppSettings, SharedSettings};
use anyhow::Result;
use core_foundation::uuid::{CFUUIDGetUUIDBytes, CFUUIDRef};
use core_graphics::display::{CGDirectDisplayID, CGDisplay};
use core_graphics::geometry::CGPoint;
use gpui::{
    AnyWindowHandle, App, AsyncApp, Bounds, Context, DisplayId, IntoElement, Render,
    SharedString, Window, WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions,
    black, div, prelude::*, px, size, white,
};
use gpui_component::{
    Root,
    button::{Button, ButtonVariants as _},
    input::{Input, InputState},
};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::Duration;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub enum UiCommand {
    ShowSettings,
    ShowOverlay { display_uuid: Option<Uuid> },
    HideOverlay,
}

pub fn spawn_ui_command_loop(
    cx: &mut App,
    settings: SharedSettings,
    ui_rx: Receiver<UiCommand>,
) {
    cx.spawn({
        let settings = settings.clone();
        async move |cx| {
            let mut controller = UiController::new(settings);

            loop {
                let mut disconnected = false;

                loop {
                    match ui_rx.try_recv() {
                        Ok(command) => {
                            if let Err(error) = controller.handle(command, cx) {
                                eprintln!("ui command failed: {error:#}");
                            }
                        }
                        Err(TryRecvError::Empty) => break,
                        Err(TryRecvError::Disconnected) => {
                            disconnected = true;
                            break;
                        }
                    }
                }

                if disconnected {
                    break;
                }

                cx.background_executor().timer(Duration::from_millis(16)).await;
            }
        }
    })
    .detach();
}

pub fn open_settings_window_now(cx: &mut App, settings: SharedSettings) -> Result<()> {
    let bounds = Bounds::centered(None, size(px(520.), px(480.)), cx);
    let initial = settings
        .read()
        .map_err(|_| anyhow::anyhow!("settings lock poisoned"))?
        .clone();

    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            ..Default::default()
        },
        move |window, cx| {
            let view = cx.new(|cx| SettingsView::new(settings, initial, window, cx));
            cx.new(|cx| Root::new(view, window, cx))
        },
    )?;

    cx.activate(true);
    Ok(())
}

pub fn display_uuid_for_point(point: CGPoint) -> Option<Uuid> {
    let (display_ids, count) = CGDisplay::displays_with_point(point, 1).ok()?;
    display_ids
        .into_iter()
        .take(count as usize)
        .next()
        .and_then(display_uuid)
}

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn CGDisplayCreateUUIDFromDisplayID(display: CGDirectDisplayID) -> CFUUIDRef;
}

fn display_uuid(display_id: CGDirectDisplayID) -> Option<Uuid> {
    let uuid_ref = unsafe { CGDisplayCreateUUIDFromDisplayID(display_id) };
    if uuid_ref.is_null() {
        return None;
    }

    let bytes = unsafe { CFUUIDGetUUIDBytes(uuid_ref) };
    Some(Uuid::from_bytes([
        bytes.byte0,
        bytes.byte1,
        bytes.byte2,
        bytes.byte3,
        bytes.byte4,
        bytes.byte5,
        bytes.byte6,
        bytes.byte7,
        bytes.byte8,
        bytes.byte9,
        bytes.byte10,
        bytes.byte11,
        bytes.byte12,
        bytes.byte13,
        bytes.byte14,
        bytes.byte15,
    ]))
}

struct UiController {
    settings: SharedSettings,
    settings_window: Option<AnyWindowHandle>,
    overlay_window: Option<AnyWindowHandle>,
}

impl UiController {
    fn new(settings: SharedSettings) -> Self {
        Self {
            settings,
            settings_window: None,
            overlay_window: None,
        }
    }

    fn handle(&mut self, command: UiCommand, cx: &mut AsyncApp) -> Result<()> {
        match command {
            UiCommand::ShowSettings => self.show_settings_window(cx),
            UiCommand::ShowOverlay { display_uuid } => self.show_overlay(display_uuid, cx),
            UiCommand::HideOverlay => self.hide_overlay(cx),
        }
    }

    fn show_settings_window(&mut self, cx: &mut AsyncApp) -> Result<()> {
        activate_menu_bar_app();

        if let Some(handle) = self.settings_window {
            if handle
                .update(cx, |_, window, _| {
                    window.activate_window();
                })
                .is_ok()
            {
                return Ok(());
            }

            self.settings_window = None;
        }

        let bounds = cx.update(|app| Bounds::centered(None, size(px(520.), px(480.)), app))?;
        let settings = self.settings.clone();
        let initial = self.current_settings()?;

        let window = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            move |window, cx| {
                let view = cx.new(|cx| SettingsView::new(settings, initial, window, cx));
                cx.new(|cx| Root::new(view, window, cx))
            },
        )?;

        self.settings_window = Some(window.into());
        Ok(())
    }

    fn show_overlay(&mut self, display_uuid: Option<Uuid>, cx: &mut AsyncApp) -> Result<()> {
        self.hide_overlay(cx)?;

        let display_id = self.resolve_display(display_uuid, cx)?;
        let bounds = cx.update(|app| {
            let display_id = display_id.or_else(|| app.primary_display().map(|display| display.id()));
            Bounds::centered(display_id, size(px(150.), px(150.)), app)
        })?;

        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            display_id,
            titlebar: None,
            window_background: WindowBackgroundAppearance::Transparent,
            focus: false,
            show: true,
            kind: WindowKind::PopUp,
            is_movable: false,
            ..Default::default()
        };

        let window = cx.open_window(options, |_, cx| cx.new(|_| ListeningOverlay))?;
        self.overlay_window = Some(window.into());

        Ok(())
    }

    fn hide_overlay(&mut self, cx: &mut AsyncApp) -> Result<()> {
        if let Some(handle) = self.overlay_window.take() {
            let _ = handle.update(cx, |_, window, _| {
                window.remove_window();
            });
        }

        Ok(())
    }

    fn current_settings(&self) -> Result<AppSettings> {
        Ok(self
            .settings
            .read()
            .map_err(|_| anyhow::anyhow!("settings lock poisoned"))?
            .clone())
    }

    fn resolve_display(
        &self,
        display_uuid: Option<Uuid>,
        cx: &mut AsyncApp,
    ) -> Result<Option<DisplayId>> {
        cx.update(|app| {
            let Some(target_uuid) = display_uuid else {
                return app.primary_display().map(|display| display.id());
            };

            app.displays()
                .into_iter()
                .find(|display| display.uuid().ok() == Some(target_uuid))
                .map(|display| display.id())
                .or_else(|| app.primary_display().map(|display| display.id()))
        })
    }
}

struct SettingsView {
    settings: SharedSettings,
    api_key_input: gpui::Entity<InputState>,
    model_input: gpui::Entity<InputState>,
    prompt_input: gpui::Entity<InputState>,
    status_message: Option<SharedString>,
    storage_path: SharedString,
}

impl SettingsView {
    fn new(
        settings: SharedSettings,
        initial: AppSettings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let api_key_input = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(initial.openrouter_api_key)
                .placeholder("OpenRouter API key")
                .masked(true)
        });
        let model_input = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(initial.openrouter_model)
                .placeholder("openai/gpt-audio-mini")
        });
        let prompt_input = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(initial.prompt)
                .placeholder("Transcription prompt")
                .multi_line(true)
                .rows(8)
        });

        Self {
            settings,
            api_key_input,
            model_input,
            prompt_input,
            status_message: None,
            storage_path: AppSettings::storage_path()
                .map(|path| path.display().to_string().into())
                .unwrap_or_else(|_| "Could not resolve settings path".into()),
        }
    }

    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut next = match self.settings.read() {
            Ok(settings) => settings.clone(),
            Err(_) => {
                self.status_message = Some("Could not access the current settings.".into());
                cx.notify();
                return;
            }
        };

        next.openrouter_api_key = self.api_key_input.read(cx).value().to_string();
        next.openrouter_model = self.model_input.read(cx).value().to_string();
        next.prompt = self.prompt_input.read(cx).value().to_string();
        next.normalize_for_save();

        if let Err(error) = next.save() {
            self.status_message = Some(format!("Could not save settings: {error}").into());
            cx.notify();
            return;
        }

        match self.settings.write() {
            Ok(mut settings) => *settings = next,
            Err(_) => {
                self.status_message = Some("Settings were saved, but the running app was not updated.".into());
                cx.notify();
                return;
            }
        }

        window.remove_window();
    }
}

impl Render for SettingsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity().clone();

        let mut content = div()
            .size_full()
            .bg(gpui::rgb(0x0d1117))
            .text_color(gpui::rgb(0xf8fafc))
            .p_6()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_5()
                    .size_full()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(div().text_xl().child("tvoice"))
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(gpui::rgb(0x94a3b8))
                                    .child("Menu-bar push-to-talk transcription for macOS."),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(div().text_sm().child("OpenRouter API key"))
                            .child(
                                Input::new(&self.api_key_input)
                                    .mask_toggle()
                                    .cleanable(true)
                                    .w_full(),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(div().text_sm().child("Model"))
                            .child(Input::new(&self.model_input).cleanable(true).w_full())
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(gpui::rgb(0x94a3b8))
                                    .child("This is used for each transcription request."),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(div().text_sm().child("Prompt"))
                            .child(Input::new(&self.prompt_input).h(px(180.)).w_full())
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(gpui::rgb(0x94a3b8))
                                    .child(
                                        "Prompt text is sent with the recorded audio to guide transcription.",
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .mt_auto()
                            .flex()
                            .flex_col()
                            .gap_4()
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(gpui::rgb(0x64748b))
                                    .child(format!("Saved in {}", self.storage_path)),
                            ),
                    ),
            );

        if let Some(message) = self.status_message.clone() {
            content = content.child(
                div()
                    .text_sm()
                    .text_color(gpui::rgb(0xfbbf24))
                    .child(message),
            );
        }

        content.child(
            div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("cancel")
                        .ghost()
                        .label("Cancel")
                        .on_click(|_, window, _| {
                            window.remove_window();
                        }),
                )
                .child(Button::new("save").primary().label("Save").on_click(
                    move |_, window, cx| {
                        let _ = view.update(cx, |this, cx| this.save(window, cx));
                    },
                )),
        )
    }
}

struct ListeningOverlay;

impl Render for ListeningOverlay {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .w(px(150.))
                    .h(px(150.))
                    .rounded_full()
                    .bg(black().opacity(0.72))
                    .border_1()
                    .border_color(white().opacity(0.18))
                    .shadow_xl()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_4()
                    .child(
                        div()
                            .flex()
                            .items_end()
                            .gap_2()
                            .h(px(34.))
                            .child(
                                div()
                                    .w(px(9.))
                                    .h(px(16.))
                                    .rounded_full()
                                    .bg(white()),
                            )
                            .child(
                                div()
                                    .w(px(9.))
                                    .h(px(32.))
                                    .rounded_full()
                                    .bg(white()),
                            )
                            .child(
                                div()
                                    .w(px(9.))
                                    .h(px(22.))
                                    .rounded_full()
                                    .bg(white()),
                            ),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(white())
                            .child("Listening"),
                    ),
            )
    }
}

fn activate_menu_bar_app() {
    unsafe {
        use cocoa::appkit::{NSApp, NSApplication};
        use cocoa::base::YES;

        let app = NSApp();
        if app != cocoa::base::nil {
            app.activateIgnoringOtherApps_(YES);
        }
    }
}
