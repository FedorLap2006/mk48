// SPDX-FileCopyrightText: 2021 Softbear, Inc.
// SPDX-License-Identifier: AGPL-3.0-or-later

// Yew `tr`
#![feature(stmt_expr_attributes)]
#![feature(int_log)]
#![feature(result_option_inspect)]
#![feature(pattern)]
#![feature(array_try_map)]

mod canvas;
pub mod component;
pub mod dialog;
mod error_tracer;
pub mod event;
pub mod frontend;
mod keyboard;
pub mod overlay;
pub mod translation;
pub mod window;

use crate::canvas::Canvas;
use crate::dialog::privacy_dialog::PrivacyDialog;
use crate::dialog::terms_dialog::TermsDialog;
use crate::error_tracer::ErrorTracer;
use crate::frontend::post_message;
use crate::overlay::fatal_error::FatalError;
use crate::overlay::reconnecting::Reconnecting;
use crate::window::event_listener::WindowEventListener;
use client_util::browser_storage::BrowserStorages;
use client_util::context::WeakCoreState;
use client_util::game_client::GameClient;
use client_util::infrastructure::Infrastructure;
use client_util::setting::CommonSettings;
use core_protocol::id::{InvitationId, ServerId};
use core_protocol::name::Referrer;
use core_protocol::rpc::{ChatRequest, PlayerRequest, Request, TeamRequest};
use engine_macros::SmolRoutable;
use frontend::{Ctw, Gctw, PropertiesWrapper, Yew};
use gloo_render::{request_animation_frame, AnimationFrame};
use js_hooks::console_log;
use keyboard::KeyboardEventsListener;
use std::marker::PhantomData;
use std::num::NonZeroU8;
use stylist::{global_style, GlobalStyle};
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::future_to_promise;
use web_sys::{FocusEvent, KeyboardEvent, MessageEvent, MouseEvent, TouchEvent, WheelEvent};
use yew::prelude::*;
use yew_router::prelude::*;

pub const CONTACT_EMAIL: &'static str = "contact@softbear.com";

struct App<G: GameClient, UI: Component<Properties = PropertiesWrapper<G::UiProps>>, R: Routable>
where
    G::UiProps: Default + PartialEq + Clone,
{
    context_menu: Option<Html>,
    infrastructure: Option<Infrastructure<G>>,
    ui_props: G::UiProps,
    fatal_error: Option<String>,
    /// After [`AppMsg::RecreateCanvas`] is received, before [`AppMsg::RecreateRenderer`] is received.
    recreating_canvas: RecreatingCanvas,
    /// Whether outbound links are enabled.
    outbound_enabled: bool,
    _animation_frame: AnimationFrame,
    _keyboard_events_listener: KeyboardEventsListener,
    _visibility_listener: WindowEventListener<Event>,
    /// Message from parent window.
    _message_listener: WindowEventListener<MessageEvent>,
    _context_menu_inhibitor: WindowEventListener<MouseEvent>,
    _error_tracer: ErrorTracer,
    _global_style: GlobalStyle,
    _spooky: PhantomData<(UI, R)>,
}

#[derive(Copy, Clone, PartialEq, Default)]
enum RecreatingCanvas {
    /// No canvas recreation is in progress.
    #[default]
    None,
    /// Canvas is removed.
    Started,
    /// Canvas is restored.
    Finished,
}

#[derive(Default, PartialEq, Properties)]
struct AppProps {}

enum AppMsg<G: GameClient> {
    ChangeCommonSettings(Box<dyn FnOnce(&mut CommonSettings, &mut BrowserStorages)>),
    ChangeSettings(Box<dyn FnOnce(&mut G::GameSettings, &mut BrowserStorages)>),
    CreateInfrastructure(Box<Infrastructure<G>>),
    /// Signals the canvas should be recreated, followed by the renderer.
    RecreateCanvas,
    /// Put back the canvas.
    #[doc(hidden)]
    RecreateCanvasPart2,
    /// Signals just the renderer should be recreated.
    RecreateRenderer,
    SetServerId(Option<ServerId>),
    FatalError(String),
    Frame {
        time: f64,
    },
    KeyboardFocus(FocusEvent),
    Keyboard(KeyboardEvent),
    MouseFocus(FocusEvent),
    Mouse(MouseEvent),
    RawZoom(f32),
    SendChatRequest(ChatRequest),
    SendPlayerRequest(PlayerRequest),
    SendTeamRequest(TeamRequest),
    SendUiEvent(G::UiEvent),
    SetContextMenuProps(Option<Html>),
    SetUiProps(G::UiProps),
    Touch(TouchEvent),
    /// Error trace.
    Trace(String),
    VisibilityChange(Event),
    /// Message from parent window.
    Message(String),
    Wheel(WheelEvent),
}

impl<
        G: GameClient,
        UI: Component<Properties = PropertiesWrapper<G::UiProps>>,
        R: Routable + 'static,
    > App<G, UI, R>
where
    G::UiProps: Default + PartialEq + Clone,
{
    pub fn create_animation_frame(ctx: &Context<Self>) -> AnimationFrame {
        let link = ctx.link().clone();
        request_animation_frame(move |time| link.send_message(AppMsg::Frame { time }))
    }
}

impl<
        G: GameClient,
        UI: Component<Properties = PropertiesWrapper<G::UiProps>>,
        R: Routable + 'static,
    > Component for App<G, UI, R>
where
    G::UiProps: Default + PartialEq + Clone,
{
    type Message = AppMsg<G>;
    type Properties = AppProps;

    fn create(ctx: &Context<Self>) -> Self {
        let keyboard_callback = ctx.link().callback(AppMsg::Keyboard);
        let keyboard_focus_callback = ctx.link().callback(AppMsg::KeyboardFocus);
        let visibility_callback = ctx.link().callback(AppMsg::VisibilityChange);
        let message_callback = ctx.link().callback(AppMsg::Message);
        let trace_callback = ctx.link().callback(AppMsg::Trace);

        Self {
            context_menu: None,
            infrastructure: None,
            ui_props: G::UiProps::default(),
            recreating_canvas: RecreatingCanvas::default(),
            fatal_error: None,
            outbound_enabled: true,
            _animation_frame: Self::create_animation_frame(ctx),
            _keyboard_events_listener: KeyboardEventsListener::new(
                keyboard_callback,
                keyboard_focus_callback,
            ),
            _visibility_listener: WindowEventListener::new(
                "visibilitychange",
                move |event: &Event| {
                    visibility_callback.emit(event.clone());
                },
                false,
            ),
            _message_listener: WindowEventListener::new(
                "message",
                move |event: &MessageEvent| {
                    let data = event.data();
                    if let Some(string) = data.as_string() {
                        message_callback.emit(string);
                    } else {
                        #[cfg(debug_assertions)]
                        console_log!(
                            "invalid message type: {:?} {:?}",
                            data.js_typeof().as_string(),
                            js_sys::JSON::stringify(&data)
                        )
                    }
                },
                false,
            ),
            _context_menu_inhibitor: WindowEventListener::new_body(
                "contextmenu",
                move |event: &MouseEvent| event.prevent_default(),
                true,
            ),
            _error_tracer: ErrorTracer::new(trace_callback),
            _global_style: global_style!(
                r#"
                html {
                    font-family: sans-serif;
                    font-size: 1.5vmin;
                    font-size: calc(7px + 0.8vmin);
                }

                body {
                    color: white;
                    margin: 0;
                    overflow: hidden;
                    padding: 0;
                    touch-action: none;
                    user-select: none;
                }

                a {
                    color: white;
                }
            "#
            )
            .expect("failed to mount global style"),
            _spooky: PhantomData,
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: AppMsg<G>) -> bool {
        match msg {
            AppMsg::ChangeCommonSettings(change) => {
                if let Some(infrastructure) = self.infrastructure.as_mut() {
                    change(
                        &mut infrastructure.context.common_settings,
                        &mut infrastructure.context.browser_storages,
                    );
                    // Just in case.
                    return true;
                }
            }
            AppMsg::ChangeSettings(change) => {
                if let Some(infrastructure) = self.infrastructure.as_mut() {
                    change(
                        &mut infrastructure.context.settings,
                        &mut infrastructure.context.browser_storages,
                    );
                    // Just in case.
                    return true;
                }
            }
            AppMsg::CreateInfrastructure(infrastructure) => {
                assert!(self.infrastructure.is_none());
                self.infrastructure = Some(*infrastructure);
            }
            AppMsg::RecreateCanvas => {
                self.recreating_canvas = RecreatingCanvas::Started;
                console_log!("started recreating canvas");
                return true;
            }
            AppMsg::RecreateCanvasPart2 => {
                self.recreating_canvas = RecreatingCanvas::Finished;
                console_log!("finished recreating canvas");
                return true;
            }
            AppMsg::RecreateRenderer => {
                self.recreating_canvas = RecreatingCanvas::None;
                if let Some(infrastructure) = self.infrastructure.as_mut() {
                    if let Err(e) = infrastructure.recreate_renderer() {
                        console_log!("could not recreate renderer: {}", e);
                    } else {
                        console_log!("finished recreating renderer");
                    }
                }
                return true;
            }
            AppMsg::SetServerId(server_id) => {
                if let Some(infrastructure) = self.infrastructure.as_mut() {
                    infrastructure.choose_server_id(server_id);
                }
            }
            AppMsg::FatalError(e) => {
                self.fatal_error = Some(e);
                return true;
            }
            AppMsg::Frame { time } => {
                if self.recreating_canvas != RecreatingCanvas::Started {
                    if let Some(infrastructure) = self.infrastructure.as_mut() {
                        infrastructure.frame((time * 0.001) as f32);
                    }
                }
                self._animation_frame = Self::create_animation_frame(ctx);
            }
            AppMsg::Keyboard(event) => {
                if let Some(infrastructure) = self.infrastructure.as_mut() {
                    infrastructure.keyboard(event);
                }
            }
            AppMsg::KeyboardFocus(event) => {
                if let Some(infrastructure) = self.infrastructure.as_mut() {
                    infrastructure.keyboard_focus(event);
                }
            }
            AppMsg::Mouse(event) => {
                if let Some(infrastructure) = self.infrastructure.as_mut() {
                    infrastructure.mouse(event);
                }
            }
            AppMsg::MouseFocus(event) => {
                if let Some(infrastructure) = self.infrastructure.as_mut() {
                    infrastructure.mouse_focus(event);
                }
            }
            AppMsg::RawZoom(amount) => {
                if let Some(infrastructure) = self.infrastructure.as_mut() {
                    infrastructure.raw_zoom(amount);
                }
            }
            AppMsg::SendChatRequest(request) => {
                if let Some(infrastructure) = self.infrastructure.as_mut() {
                    infrastructure.send_request(Request::Chat(request));
                }
            }
            AppMsg::SetContextMenuProps(props) => {
                self.context_menu = props;
                return true;
            }
            AppMsg::SendPlayerRequest(request) => {
                if let Some(infrastructure) = self.infrastructure.as_mut() {
                    infrastructure.send_request(Request::Player(request));
                }
            }
            AppMsg::SendTeamRequest(request) => {
                if let Some(infrastructure) = self.infrastructure.as_mut() {
                    infrastructure.send_request(Request::Team(request));
                }
            }
            AppMsg::SendUiEvent(event) => {
                if let Some(infrastructure) = self.infrastructure.as_mut() {
                    infrastructure.ui_event(event);
                }
            }
            AppMsg::SetUiProps(props) => {
                self.ui_props = props;
                return true;
            }
            AppMsg::Touch(event) => {
                if let Some(infrastructure) = self.infrastructure.as_mut() {
                    infrastructure.touch(event);
                }
            }
            AppMsg::Trace(message) => {
                if let Some(infrastructure) = self.infrastructure.as_mut() {
                    infrastructure.trace(message);
                }
            }
            AppMsg::VisibilityChange(event) => {
                if let Some(infrastructure) = self.infrastructure.as_mut() {
                    infrastructure.visibility_change(event);
                }
            }
            AppMsg::Message(message) => {
                console_log!("received message: {}", message);
                match message.as_str() {
                    "snippetLoaded" => {
                        post_message("gameLoaded");
                    }
                    "enableOutbound" => {
                        self.outbound_enabled = true;
                        return true;
                    }
                    "disableOutbound" => {
                        self.outbound_enabled = false;
                        return true;
                    }
                    #[cfg(feature = "audio")]
                    "mute" => {
                        if let Some(infrastructure) = self.infrastructure.as_mut() {
                            infrastructure.context.audio.set_muted_by_ad(true);
                        }
                    }
                    #[cfg(feature = "audio")]
                    "unmute" => {
                        if let Some(infrastructure) = self.infrastructure.as_mut() {
                            infrastructure.context.audio.set_muted_by_ad(false);
                        }
                    }
                    _ => {}
                }
            }
            AppMsg::Wheel(event) => {
                if let Some(infrastructure) = self.infrastructure.as_mut() {
                    infrastructure.wheel(event);
                }
            }
        }
        false
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let change_common_settings_callback = ctx.link().callback(AppMsg::ChangeCommonSettings);
        let change_settings_callback = ctx.link().callback(AppMsg::ChangeSettings);
        let chat_request_callback = ctx.link().callback(AppMsg::SendChatRequest);
        let player_request_callback = ctx.link().callback(AppMsg::SendPlayerRequest);
        let raw_zoom_callback = ctx.link().callback(AppMsg::RawZoom);
        let recreate_renderer_callback = ctx.link().callback(|_| AppMsg::RecreateCanvas);
        let set_server_id_callback = ctx.link().callback(AppMsg::SetServerId);
        let send_ui_event_callback = ctx.link().callback(AppMsg::SendUiEvent);
        let set_context_menu_callback = ctx.link().callback(AppMsg::SetContextMenuProps);
        let team_request_callback = ctx.link().callback(AppMsg::SendTeamRequest);

        // Combine game and engine routes, except those with path parameters.
        let routes = R::routes()
            .into_iter()
            .chain(Route::routes().into_iter())
            .filter(|r| !r.contains(':') && r.chars().filter(|&c| c == '/').count() == 2)
            .collect::<Vec<_>>();

        let context = Ctw {
            chat_request_callback,
            change_common_settings_callback,
            game_id: G::GAME_ID,
            outbound_enabled: self.outbound_enabled,
            player_request_callback,
            raw_zoom_callback,
            recreate_renderer_callback,
            set_server_id_callback,
            set_context_menu_callback,
            routes,
            setting_cache: self
                .infrastructure
                .as_ref()
                .map(|i| i.context.common_settings.clone())
                .unwrap_or_default(),
            state: self
                .infrastructure
                .as_ref()
                .map(|i| WeakCoreState::new(&i.context.state.core))
                .unwrap_or_default(),
            team_request_callback,
        };

        let game_context = Gctw {
            send_ui_event_callback,
            settings_cache: self
                .infrastructure
                .as_ref()
                .map(|i| i.context.settings.clone())
                .unwrap_or_default(),
            change_settings_callback,
        };

        html! {
            <BrowserRouter>
                <ContextProvider<Ctw> {context}>
                    <ContextProvider<Gctw<G>> context={game_context}>
                        if self.recreating_canvas != RecreatingCanvas::Started {
                            <Canvas
                                resolution_divisor={NonZeroU8::new(1).unwrap()}
                                mouse_callback={ctx.link().callback(AppMsg::Mouse)}
                                touch_callback={ctx.link().callback(AppMsg::Touch)}
                                focus_callback={ctx.link().callback(AppMsg::MouseFocus)}
                                wheel_callback={ctx.link().callback(AppMsg::Wheel)}
                            />
                        }
                        if self.infrastructure.as_ref().map(|i| i.context.connection_lost()).unwrap_or_default() {
                            <FatalError/>
                        } else if let Some(message) = self.fatal_error.as_ref() {
                            <FatalError message={message.to_owned()}/>
                        } else {
                            <>
                                <UI props={self.ui_props.clone()}/>
                                <Switch<Route> render={Switch::render(switch)}/>
                                if let Some(context_menu) = self.context_menu.as_ref() {
                                    {context_menu.clone()}
                                }
                                if self.infrastructure.as_ref().map(|i| i.context.socket.is_reconnecting()).unwrap_or_default() {
                                    <Reconnecting/>
                                }
                            </>
                        }
                    </ContextProvider<Gctw<G>>>
                </ContextProvider<Ctw>>
            </BrowserRouter>
        }
    }

    fn rendered(&mut self, ctx: &Context<Self>, first_render: bool) {
        if first_render {
            let set_ui_props = ctx.link().callback(AppMsg::SetUiProps);
            let create_infrastructure_callback = ctx.link().callback(AppMsg::CreateInfrastructure);
            let fatal_error_callback = ctx.link().callback(AppMsg::FatalError);
            let _ = future_to_promise(async move {
                match Infrastructure::new(Box::new(Yew::new(set_ui_props).await)) {
                    Ok(infrastructure) => {
                        create_infrastructure_callback.emit(Box::new(infrastructure))
                    }
                    Err(e) => fatal_error_callback.emit(e),
                }
                Ok(JsValue::NULL)
            });
        }
        match self.recreating_canvas {
            RecreatingCanvas::None => {}
            RecreatingCanvas::Started => ctx.link().send_message(AppMsg::RecreateCanvasPart2),
            RecreatingCanvas::Finished => ctx.link().send_message(AppMsg::RecreateRenderer),
        }
    }
}

pub fn entry_point<
    G: GameClient,
    UI: Component<Properties = PropertiesWrapper<G::UiProps>>,
    R: Routable + 'static,
>()
where
    G::UiProps: Default + PartialEq + Clone,
{
    yew::start_app::<App<G, UI, R>>();
}

#[derive(Clone, Copy, PartialEq, SmolRoutable)]
pub enum Route {
    #[at("/invite/:invitation_id/")]
    Invitation { invitation_id: InvitationId },
    #[at("/referrer/:referrer/")]
    Referrer { referrer: Referrer },
    #[at("/privacy/")]
    Privacy,
    #[at("/terms/")]
    Terms,
    #[not_found]
    #[at("/")]
    Home,
}

fn switch(routes: &Route) -> Html {
    match routes {
        Route::Home | Route::Invitation { .. } | Route::Referrer { .. } => html! {},
        Route::Privacy => html! {
            <PrivacyDialog/>
        },
        Route::Terms => html! {
            <TermsDialog/>
        },
    }
}
