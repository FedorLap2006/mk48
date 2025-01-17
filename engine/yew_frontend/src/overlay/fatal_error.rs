// SPDX-FileCopyrightText: 2021 Softbear, Inc.
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::component::positioner::{Position, Positioner};
use crate::translation::{t, Translation};
use stylist::yew::styled_component;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::{future_to_promise, JsFuture};
use web_sys::{window, Request, RequestInit, RequestMode, Response};
use yew::virtual_dom::AttrValue;
use yew::{classes, html, Properties};

#[derive(Properties, PartialEq)]
pub struct FatalErrorProps {
    pub message: Option<AttrValue>,
}

#[styled_component(FatalError)]
pub fn fatal_error(props: &FatalErrorProps) -> Html {
    let container_style = css!(
        r#"
        background-color: #f6f6f6;
		border-radius: 1rem;
		box-shadow: 0em 0.25rem 0 #cccccc;
		color: #000000;
		font-size: 2rem;
		word-break: break-word;
        "#
    );

    let p_css = css!(
        r#"
        margin: 1rem;
        "#
    );

    let button_css = css! {
        r#"
        background-color: #549f57;
        border-radius: 1rem;
        border: 1px solid #61b365;
        box-sizing: border-box;
        color: white;
        cursor: pointer;
        font-size: 2rem;
        margin: 1rem;
        min-width: 12rem;
        padding-bottom: 0.7rem;
        padding-top: 0.5rem;
        text-decoration: none;
        white-space: nowrap;
        width: min-content;

        :disabled {
            filter: opacity(0.6);
        }

        :hover:not(:disabled) {
            filter: brightness(0.95);
        }

        :active:not(:disabled) {
            filter: brightness(0.9);
        }
        "#
    };

    // Refresh the page, which serves two purposes:
    // - The server may have restarted, so might need to download new client
    // - The refreshed client will attempt to regain connection
    let refresh = |_| {
        let _ = future_to_promise(async {
            // Do a pre-flight request to make sure we aren't refreshing ourselves into a browser error.
            let mut opts = RequestInit::new();
            opts.method("GET");
            opts.mode(RequestMode::Cors);

            let request = match Request::new_with_str_and_init("/", &opts) {
                Ok(request) => request,
                Err(_) => return Err(JsValue::NULL),
            };
            let window = window().unwrap();
            let response_value = match JsFuture::from(window.fetch_with_request(&request)).await {
                Ok(response_value) => response_value,
                Err(_) => return Err(JsValue::NULL),
            };
            let response: Response = match response_value.dyn_into() {
                Ok(response) => response,
                Err(_) => return Err(JsValue::NULL),
            };
            if response.ok() {
                let _ = window.location().reload();
            }
            Ok(JsValue::NULL)
        });
    };

    html! {
        <Positioner id="fatal_error" position={Position::Center} class={classes!(container_style)}>
            <p class={p_css}>{props.message.clone().unwrap_or(t().connection_lost_message().into())}</p>
            <button onclick={refresh} class={button_css}>{"Refresh"}</button>
        </Positioner>
    }
}
