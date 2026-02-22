use wasmtime::component::*;
use wasmtime::{Config, Engine, Store};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, ResourceTable, WasiView, WasiCtxView};
use fjall::Keyspace;
use wasmtime_wasi::p2::{add_to_linker_sync, add_to_linker_async};
use teloxide::types::{Message, ChatKind};

bindgen!({
    world: "plugin",
    path: "wit",
    exports: {
        default: async
    },
    imports: {
        default: async
    }
});

pub use self::exports::local::sinner_saint::handler::{
    TelegramUser, ChatContext, TriggerEvent, PluginResponse
};

struct MyState {
    ctx: WasiCtx,
    table: ResourceTable,
    db: Keyspace,
}

impl WasiView for MyState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView { ctx: &mut self.ctx, table: &mut self.table }
    }
}

impl local::sinner_saint::db_storage::Host for MyState {
    async fn get_state(&mut self, key: String) -> Option<String> {
        self.db.get(key).ok().flatten()
            .map(|v| String::from_utf8_lossy(&v).into_owned())
    }
    async fn set_state(&mut self, key: String, value: String) {
        let _ = self.db.insert(key, value);
    }
    async fn delete_state(&mut self, key: String) {
        let _ = self.db.remove(key);
    }
}

pub struct WasmHost {
    engine: Engine,
    component: Component,
    linker: Linker<MyState>,
    db: Keyspace
}

impl WasmHost {
    pub async fn new(wasm_path: &str, db: Keyspace) -> anyhow::Result<Self> {
        let mut config = Config::new();
        config.wasm_component_model(true);
        config.async_support(true);

        let engine = Engine::new(&config)?;
        let component = Component::from_file(&engine, wasm_path)?;
        let mut linker = Linker::new(&engine);

        add_to_linker_async(&mut linker)?;

        Plugin::add_to_linker::<MyState, wasmtime::component::HasSelf<MyState>>(
            &mut linker,
            |s| s
        )?;

        Ok(Self { engine, component, linker, db })
    }

    pub async fn run(&self, input: &str) -> anyhow::Result<String> {
        let mut builder = WasiCtxBuilder::new();
        builder.inherit_stdio();
        let ctx = builder.build();

        let state = MyState {
            ctx,
            table: ResourceTable::new(),
            db: self.db.clone()
        };

        let mut store = Store::new(&self.engine, state);
        let plugin = Plugin::instantiate_async(&mut store, &self.component, &self.linker).await?;

        // let result = plugin.interface0.call_process_message(&mut store, input)?;
        let result = plugin.interface0.call_process_message(&mut store, input).await?;

        Ok(result)
    }

    pub fn map_tele_to_wit(&self, msg: &Message) -> (TelegramUser, ChatContext) {
        let from = msg.from.as_ref().expect("Message must have a sender");

        let user = TelegramUser {
            id: from.id.0 as i64,
            username: from.username.clone(),
            is_bot: from.is_bot,
            language_code: from.language_code.clone(),
        };

        let chat = ChatContext {
            id: msg.chat.id.0,
            chat_type: match &msg.chat.kind {
                ChatKind::Private(_) => "private".to_string(),
                ChatKind::Public(c) => format!("{:?}", c.kind),
            },
            title: msg.chat.title().map(|s| s.to_string()),
        };

        (user, chat)
    }
    pub fn map_tele_to_wit_from_user(&self, from: &teloxide::types::User) -> (TelegramUser, ChatContext) {
        let user = TelegramUser {
            id: from.id.0 as i64,
            username: from.username.clone(),
            is_bot: from.is_bot,
            language_code: from.language_code.clone(),
        };

        let chat = ChatContext {
            id: from.id.0 as i64,
            chat_type: "private".to_string(),
            title: None,
        };

        (user, chat)
    }

    pub async fn dispatch(
        &self,
        user: TelegramUser,
        chat: ChatContext,
        event: TriggerEvent,
        timestamp: u64
    ) -> anyhow::Result<PluginResponse> {
        let mut builder = WasiCtxBuilder::new();
        builder.inherit_stdio();
        let ctx = builder.build();

        let state = MyState {
            ctx,
            table: ResourceTable::new(),
            db: self.db.clone(),
        };

        let mut store = Store::new(&self.engine, state);
        // let plugin = Plugin::instantiate(&mut store, &self.component, &self.linker)?;
        let plugin = Plugin::instantiate_async(&mut store, &self.component, &self.linker).await?;

        // let response = plugin.interface0.call_process_event(&mut store, &user, &chat, &event, timestamp)?;
        let response = plugin
                .local_sinner_saint_handler() // This path matches your WIT interface
                .call_process_event(&mut store, &user, &chat, &event, timestamp)
                .await?;

        Ok(response)
    }
}
