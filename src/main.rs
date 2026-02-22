mod wasm_host;
use std::time::{SystemTime, UNIX_EPOCH};

use std::sync::Arc;
use wasm_host::{WasmHost, TriggerEvent, PluginResponse};

use dotenvy::dotenv;
use teloxide::prelude::*;
use teloxide::types::{
    InlineQueryResult, InlineQueryResultArticle, InputMessageContent, InputMessageContentText, InlineQueryResultsButton, InlineQueryResultsButtonKind,
    Me, InlineKeyboardMarkup, InlineKeyboardButton, CallbackQuery, ChosenInlineResult
};
use fjall::{Database, KeyspaceCreateOptions};
use serde::Deserialize;


#[derive(Deserialize)]
struct WasmInlineItem {
    id: String,
    title: String,
    message: String,
    button_text: Option<String>,
    button_data: Option<String>
}


#[tokio::main]
async fn main() {
    dotenv().ok();
    pretty_env_logger::init();

    let db = Database::builder("db_data")
        .cache_size(64 * 1024 * 1024)
        .max_cached_files(Some(64))
        .open()
        .expect("Failed to open database");

    let plugin_state = db.keyspace("plugin_state", KeyspaceCreateOptions::default).unwrap();

    log::info!("Starting Sinner-Saint Platform...");

    let bot = Bot::from_env();

    let wasm_runtime = Arc::new(
        WasmHost::new("plugins/logic1.wasm", plugin_state)
            .await.expect("Failed to load WASM plugin")
    );

    let inline_handler = Update::filter_inline_query()
        .endpoint(|bot: Bot, q: InlineQuery, host: Arc<WasmHost>| async move {
            let (user, chat) = host.map_tele_to_wit_from_user(&q.from);

            let event = TriggerEvent::InlineQuery(q.query.clone());

            match host.dispatch(user, chat, event, 0).await {
                Ok(PluginResponse::Inline(resp)) => {
                    let results = results_from_json(&resp.results_json);

                    let mut answer = bot.answer_inline_query(q.id, results);

                    if let Some(pm) = resp.switch_pm {
                        let button = InlineQueryResultsButton {
                            text: pm.text,
                            kind: InlineQueryResultsButtonKind::StartParameter(pm.start_parameter),
                        };
                        answer = answer.button(button);
                    }

                    if let Some(cache) = resp.cache_time {
                        answer = answer.cache_time(cache);
                    }

                    answer.await?;
                }
                _ => {
                    bot.answer_inline_query(q.id, vec![]).await?;
                }
            }
            respond(())
        });

    let message_handler = Update::filter_message()
        .endpoint(move |bot: Bot, msg: Message, me: Me, host: Arc<WasmHost>| {
            async move {
                let bot_name = me.username();
                let text = msg.text().unwrap_or("");

                let is_command = text.starts_with('/');
                let is_mention = text.contains(&format!("@{}", bot_name));
                let is_reply_to_me = msg.reply_to_message()
                    .and_then(|m| m.from.clone())
                    .map_or(false, |u| u.id == me.id);
                let is_private = msg.chat.is_private();

                if is_command || is_mention || is_reply_to_me || is_private {
                    if let Err(e) = handle_telegram_update(host, msg, bot).await {
                        log::error!("WASM Dispatch Error: {}", e);
                    }
                }
                respond(())
            }
        });
    let callback_handler = Update::filter_callback_query()
        .endpoint(move |bot: Bot, q: CallbackQuery, host: Arc<WasmHost>| async move {
            if let Some(data) = q.data.clone() {

                let (user, chat) = host.map_tele_to_wit_from_user(&q.from);

                let event = TriggerEvent::CallbackQuery(data);
                let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();

                match host.dispatch(user, chat, event, timestamp).await {
                    Ok(PluginResponse::EditInline(new_text)) => {
                        if let Some(inline_id) = q.inline_message_id {
                            let _ = bot.edit_message_text_inline(inline_id, new_text).await;
                        } else if let Some(msg) = q.regular_message() {
                            let _ = bot.edit_message_text(msg.chat.id, msg.id, new_text).await;
                        }

                        let _ = bot.answer_callback_query(q.id).await;
                    }
                    Ok(PluginResponse::Text(alert_msg)) => {
                        match bot.answer_callback_query(q.id.clone())
                            .show_alert(true)
                            .text(alert_msg.clone())
                            .await
                        {
                            Ok(_) => println!("✨ Alert shown successfully!"),
                            Err(e) => {
                                println!("❌ Telegram rejected the alert: {}", e);
                                // Fallback: If the pop-up fails, just drop a normal message in the chat
                                let _ = bot.send_message(q.message.unwrap().chat().id, alert_msg).await;
                            }
                        }
                        // let _ = bot.answer_callback_query(q.id)
                        //     .text(alert_msg)
                        //     .show_alert(true) // Makes it a pop-up window instead of a tiny top banner
                        //     .await;
                    }
                    _ => {
                        let _ = bot.answer_callback_query(q.id).await;
                    }
                }

                // if let Ok(PluginResponse::EditInline(new_text)) = host.dispatch(user, chat, event, timestamp).await {
                //     if let Some(inline_id) = q.inline_message_id {
                //         bot.edit_message_text_inline(inline_id, new_text).await?;
                //     }
                // }
            }
            respond(())
        });
    let chosen_handler = Update::filter_chosen_inline_result()
        .endpoint(|bot: Bot, q: ChosenInlineResult, host: Arc<WasmHost>| async move {
            let event = TriggerEvent::ChosenInlineResult(q.result_id.clone());
            let (user, chat) = host.map_tele_to_wit_from_user(&q.from);
            let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();

            // ⚡️ Send the click event to WASM
            if let Ok(PluginResponse::EditInline(new_text)) = host.dispatch(user, chat, event, timestamp).await {
                println!("🎯 RESSSS: {}, {:#?}", new_text, q);
                if let Some(inline_message_id) = q.inline_message_id {
                    let empty_keyboard = InlineKeyboardMarkup::default();
                    let _ = bot.edit_message_text_inline(inline_message_id, new_text)
                        .reply_markup(empty_keyboard)
                        .await;
                }
            }
            respond(())
        });

    let handler = dptree::entry()
        .branch(inline_handler)
        .branch(callback_handler)
        .branch(chosen_handler)
        .branch(message_handler);

    Dispatcher::builder(bot, handler)
        .dependencies(dptree::deps![wasm_runtime])
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;
}


pub async fn handle_telegram_update(
    host: Arc<WasmHost>,
    msg: Message,
    bot: Bot
) -> anyhow::Result<()> {
    let (user, chat) = host.map_tele_to_wit(&msg);
    let text = msg.text().unwrap_or("");

    let event = if text.starts_with('/') {
        let mut parts = text[1..].split_whitespace();
        let cmd = parts.next().unwrap_or("").to_string();
        let args = parts.map(|s| s.to_string()).collect();
        TriggerEvent::Command((cmd, args))
    } else {
        TriggerEvent::Message(text.to_string())
    };
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_secs();

    let response = host.dispatch(user, chat, event, timestamp).await?;

    match response {
        PluginResponse::Text(reply) => {
            bot.send_message(msg.chat.id, reply).await?;
        }
        PluginResponse::Inline(inline) => {
            log::info!("Plugin requested inline transition: {:?}", inline.switch_pm);
        }
        _ => {}
    }

    Ok(())
}

fn make_result(item: &WasmInlineItem) -> InlineQueryResult {
    let mut article = InlineQueryResultArticle::new(
        &item.id,
        &item.title,
        InputMessageContent::Text(InputMessageContentText::new(&item.message)),
    );

    if let (Some(text), Some(data)) = (&item.button_text, &item.button_data) {
        let keyboard = InlineKeyboardMarkup::new(vec![vec![
            InlineKeyboardButton::callback(text, data)
        ]]);
        article = article.reply_markup(keyboard);
    }

    InlineQueryResult::Article(article)
}

fn results_from_json(json_str: &str) -> Vec<InlineQueryResult> {
    serde_json::from_str::<Vec<WasmInlineItem>>(json_str)
        .unwrap_or_default()
        .into_iter()
        .map(|item| make_result(&item))
        .collect()
}
