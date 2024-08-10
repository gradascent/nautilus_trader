// -------------------------------------------------------------------------------------------------
//  Copyright (C) 2015-2024 Nautech Systems Pty Ltd. All rights reserved.
//  https://nautechsystems.io
//
//  Licensed under the GNU Lesser General Public License Version 3.0 (the "License");
//  You may not use this file except in compliance with the License.
//  You may obtain a copy of the License at https://www.gnu.org/licenses/lgpl-3.0.en.html
//
//  Unless required by applicable law or agreed to in writing, software
//  distributed under the License is distributed on an "AS IS" BASIS,
//  WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
//  See the License for the specific language governing permissions and
//  limitations under the License.
// -------------------------------------------------------------------------------------------------

//! Provides a generic `DataEngine` for all environments.

// Under development
#![allow(dead_code)]
#![allow(unused_variables)]

pub mod runner;

use std::{
    any::Any,
    cell::RefCell,
    collections::{HashMap, HashSet},
    ops::Deref,
    rc::Rc,
    sync::Arc,
};

use indexmap::IndexMap;
use nautilus_common::{
    cache::Cache,
    clock::Clock,
    logging::{RECV, RES},
    messages::data::{DataRequest, DataResponse, SubscriptionCommand},
    msgbus::{handler::MessageHandler, MessageBus},
};
use nautilus_model::{
    data::{
        bar::{Bar, BarType},
        delta::OrderBookDelta,
        deltas::OrderBookDeltas,
        depth::OrderBookDepth10,
        quote::QuoteTick,
        trade::TradeTick,
        Data, DataType,
    },
    identifiers::{ClientId, InstrumentId, Venue},
    instruments::{any::InstrumentAny, synthetic::SyntheticInstrument},
};
use ustr::Ustr;

use crate::{aggregation::BarAggregator, client::DataClientAdapter};

pub struct DataEngineConfig {
    pub time_bars_build_with_no_updates: bool,
    pub time_bars_timestamp_on_close: bool,
    pub time_bars_interval_type: String, // Make this an enum `BarIntervalType`
    pub validate_data_sequence: bool,
    pub buffer_deltas: bool,
    pub external_clients: Option<Vec<ClientId>>,
    pub debug: bool,
}

impl Default for DataEngineConfig {
    fn default() -> Self {
        Self {
            time_bars_build_with_no_updates: true,
            time_bars_timestamp_on_close: true,
            time_bars_interval_type: "left_open".to_string(), // Make this an enum `BarIntervalType`
            validate_data_sequence: false,
            buffer_deltas: false,
            external_clients: None,
            debug: false,
        }
    }
}

pub struct DataEngine {
    clock: Box<dyn Clock>,
    cache: Rc<RefCell<Cache>>,
    msgbus: Rc<RefCell<MessageBus>>,
    clients: IndexMap<ClientId, DataClientAdapter>,
    default_client: Option<DataClientAdapter>,
    routing_map: IndexMap<Venue, ClientId>,
    // order_book_intervals: HashMap<(InstrumentId, usize), Vec<fn(&OrderBook)>>,  // TODO
    bar_aggregators: Vec<Box<dyn BarAggregator>>, // TODO: dyn for now
    synthetic_quote_feeds: HashMap<InstrumentId, Vec<SyntheticInstrument>>,
    synthetic_trade_feeds: HashMap<InstrumentId, Vec<SyntheticInstrument>>,
    buffered_deltas_map: HashMap<InstrumentId, Vec<OrderBookDelta>>,
    config: DataEngineConfig,
}

impl DataEngine {
    #[must_use]
    pub fn new(
        clock: Box<dyn Clock>,
        cache: Rc<RefCell<Cache>>,
        msgbus: Rc<RefCell<MessageBus>>,
        config: Option<DataEngineConfig>,
    ) -> Self {
        Self {
            clock,
            cache,
            msgbus,
            clients: IndexMap::new(),
            routing_map: IndexMap::new(),
            default_client: None,
            bar_aggregators: Vec::new(),
            synthetic_quote_feeds: HashMap::new(),
            synthetic_trade_feeds: HashMap::new(),
            buffered_deltas_map: HashMap::new(),
            config: config.unwrap_or_default(),
        }
    }
}

impl DataEngine {
    // pub fn register_catalog(&mut self, catalog: ParquetDataCatalog) {}  TODO: Implement catalog

    /// Register the given data `client` with the engine as the default routing client.
    ///
    /// When a specific venue routing cannot be found, this client will receive messages.
    ///
    /// # Warnings
    ///
    /// Any existing default routing client will be overwritten.
    /// TODO: change this to suit message bus behaviour
    pub fn register_default_client(&mut self, client: DataClientAdapter) {
        log::info!("Registered default client {}", client.client_id());
        self.default_client = Some(client);
    }

    pub fn start(self) {
        self.clients.values().for_each(|client| client.start());
    }

    pub fn stop(self) {
        self.clients.values().for_each(|client| client.stop());
    }

    pub fn reset(self) {
        self.clients.values().for_each(|client| client.reset());
    }

    pub fn dispose(mut self) {
        self.clients.values().for_each(|client| client.dispose());
        self.clock.cancel_timers();
    }

    pub fn connect(&self) {
        todo!() //  Implement actual client connections for a live/sandbox context
    }

    pub fn disconnect(&self) {
        todo!() // Implement actual client connections for a live/sandbox context
    }

    #[must_use]
    pub fn check_connected(&self) -> bool {
        self.clients.values().all(|client| client.is_connected())
    }

    #[must_use]
    pub fn check_disconnected(&self) -> bool {
        self.clients.values().all(|client| !client.is_connected())
    }

    #[must_use]
    pub fn registed_clients(&self) -> Vec<ClientId> {
        self.clients.keys().copied().collect()
    }

    // -- SUBSCRIPTIONS ---------------------------------------------------------------------------

    fn collect_subscriptions<F, T>(&self, get_subs: F) -> Vec<T>
    where
        F: Fn(&DataClientAdapter) -> &HashSet<T>,
        T: Clone,
    {
        let mut subs = Vec::new();
        for client in self.clients.values() {
            subs.extend(get_subs(client).iter().cloned());
        }
        subs
    }

    #[must_use]
    pub fn subscribed_custom_data(&self) -> Vec<DataType> {
        self.collect_subscriptions(|client| &client.subscriptions_generic)
    }

    #[must_use]
    pub fn subscribed_instruments(&self) -> Vec<InstrumentId> {
        self.collect_subscriptions(|client| &client.subscriptions_instrument)
    }

    #[must_use]
    pub fn subscribed_order_book_deltas(&self) -> Vec<InstrumentId> {
        self.collect_subscriptions(|client| &client.subscriptions_order_book_delta)
    }

    #[must_use]
    pub fn subscribed_order_book_snapshots(&self) -> Vec<InstrumentId> {
        self.collect_subscriptions(|client| &client.subscriptions_order_book_snapshot)
    }

    #[must_use]
    pub fn subscribed_quote_ticks(&self) -> Vec<InstrumentId> {
        self.collect_subscriptions(|client| &client.subscriptions_quote_tick)
    }

    #[must_use]
    pub fn subscribed_trade_ticks(&self) -> Vec<InstrumentId> {
        self.collect_subscriptions(|client| &client.subscriptions_trade_tick)
    }

    #[must_use]
    pub fn subscribed_bars(&self) -> Vec<BarType> {
        self.collect_subscriptions(|client| &client.subscriptions_bar)
    }

    #[must_use]
    pub fn subscribed_instrument_status(&self) -> Vec<InstrumentId> {
        self.collect_subscriptions(|client| &client.subscriptions_instrument_status)
    }

    #[must_use]
    pub fn subscribed_instrument_close(&self) -> Vec<InstrumentId> {
        self.collect_subscriptions(|client| &client.subscriptions_instrument_close)
    }

    pub fn on_start(self) {
        todo!()
    }

    pub fn on_stop(self) {
        todo!()
    }

    /// Registers a new [`DataClientAdapter`]
    pub fn register_client(&mut self, client: DataClientAdapter, routing: Option<Venue>) {
        if let Some(routing) = routing {
            self.routing_map.insert(routing, client.client_id());
            log::info!("Set client {} routing for {routing}", client.client_id());
        }

        log::info!("Registered client {}", client.client_id());
        self.clients.insert(client.client_id, client);
    }

    /// Deregisters a [`DataClientAdapter`]
    pub fn deregister_client(&mut self, client_id: &ClientId) {
        // TODO: We could return a `Result` but then this is part of system wiring and instead of
        // propagating results all over the place it may be cleaner to just immediately fail
        // for these sorts of design-time errors?
        // correctness::check_key_in_map(&client_id, &self.clients, "client_id", "clients").unwrap();

        self.clients.shift_remove(client_id);
        log::info!("Deregistered client {client_id}");
    }

    fn get_client(&self, client_id: &ClientId, venue: &Venue) -> Option<&DataClientAdapter> {
        match self.clients.get(client_id) {
            Some(client) => Some(client),
            None => self
                .routing_map
                .get(venue)
                .and_then(|client_id: &ClientId| self.clients.get(client_id)),
        }
    }

    /// Send a [`DataRequest`] to an endpoint that must be a data client implementation.
    pub fn execute(&mut self, msg: &dyn Any) {
        if let Some(cmd) = msg.downcast_ref::<SubscriptionCommand>() {
            if let Some(client) = self.clients.get_mut(&cmd.client_id) {
                client.execute(cmd.clone());
            } else {
                log::error!(
                    "Cannot handle command: no client found for {}",
                    cmd.client_id
                );
            }
        } else {
            log::error!("Invalid message type received: {msg:?}");
        }
    }

    pub fn request(&self, req: DataRequest) {
        if let Some(client) = self.clients.get(&req.client_id) {
            // TODO: We don't immediately need the response
            let _ = client.request(req);
        } else {
            log::error!(
                "Cannot handle request: no client found for {}",
                req.client_id
            );
        }
    }

    /// TODO: Probably not required
    /// Send a [`SubscriptionCommand`] to an endpoint that must be a data client implementation.
    pub fn send_subscription_command(&self, message: SubscriptionCommand) {
        if let Some(client) = self.get_client(&message.client_id, &message.venue) {
            client.through_execute(message);
        }
    }

    pub fn process(&mut self, data: Data) {
        match data {
            Data::Delta(delta) => self.handle_delta(delta),
            Data::Deltas(deltas) => self.handle_deltas(deltas.deref().clone()), // TODO: Optimize
            Data::Depth10(depth) => self.handle_depth10(depth),
            Data::Quote(quote) => self.handle_quote(quote),
            Data::Trade(trade) => self.handle_trade(trade),
            Data::Bar(bar) => self.handle_bar(bar),
        }
    }

    pub fn response(&self, resp: DataResponse) {
        log::debug!("{}", format!("{RECV}{RES} response")); // TODO: Display for response

        match resp.data_type.type_name() {
            stringify!(InstrumentAny) => {
                let instruments = Arc::downcast::<Vec<InstrumentAny>>(resp.data.clone())
                    .expect("Invalid response data");
                self.handle_instruments(instruments);
            }
            stringify!(QuoteTick) => {
                let quotes = Arc::downcast::<Vec<QuoteTick>>(resp.data.clone())
                    .expect("Invalid response data");
                self.handle_quotes(quotes);
            }
            stringify!(TradeTick) => {
                let trades = Arc::downcast::<Vec<TradeTick>>(resp.data.clone())
                    .expect("Invalid response data");
                self.handle_trades(trades);
            }
            stringify!(Bar) => {
                let bars =
                    Arc::downcast::<Vec<Bar>>(resp.data.clone()).expect("Invalid response data");
                self.handle_bars(bars);
            }
            _ => {} // Nothing else to handle
        }

        self.msgbus.as_ref().borrow().send_response(resp);
    }

    // -- DATA HANDLERS ---------------------------------------------------------------------------

    // TODO: Fix all handlers to not use msgbus
    fn handle_instrument(&mut self, instrument: InstrumentAny) {
        if let Err(e) = self
            .cache
            .as_ref()
            .borrow_mut()
            .add_instrument(instrument.clone())
        {
            log::error!("Error on cache insert: {e}");
        }

        let mut msgbus = self.msgbus.borrow_mut();
        let topic = msgbus.switchboard.get_instrument_topic(instrument.id());
        msgbus.publish(&topic, &instrument as &dyn Any); // TODO: Optimize
    }

    fn handle_delta(&mut self, delta: OrderBookDelta) {
        // TODO: Manage buffered deltas
        // TODO: Manage book

        let mut msgbus = self.msgbus.borrow_mut();
        let topic = msgbus.switchboard.get_delta_topic(delta.instrument_id);
        msgbus.publish(&topic, &delta as &dyn Any); // TODO: Optimize
    }

    fn handle_deltas(&mut self, deltas: OrderBookDeltas) {
        // TODO: Manage book

        let mut msgbus = self.msgbus.borrow_mut();
        let topic = msgbus.switchboard.get_deltas_topic(deltas.instrument_id);
        msgbus.publish(&topic, &deltas as &dyn Any); // TODO: Optimize
    }

    fn handle_depth10(&mut self, depth: OrderBookDepth10) {
        // TODO: Manage book

        let mut msgbus = self.msgbus.borrow_mut();
        let topic = msgbus.switchboard.get_depth_topic(depth.instrument_id);
        msgbus.publish(&topic, &depth as &dyn Any); // TODO: Optimize
    }

    fn handle_quote(&mut self, quote: QuoteTick) {
        if let Err(e) = self.cache.as_ref().borrow_mut().add_quote(quote) {
            log::error!("Error on cache insert: {e}");
        }

        // TODO: Handle synthetics

        let mut msgbus = self.msgbus.borrow_mut();
        let topic = msgbus.switchboard.get_quote_topic(quote.instrument_id);
        msgbus.publish(&topic, &quote as &dyn Any); // TODO: Optimize
    }

    fn handle_trade(&mut self, trade: TradeTick) {
        if let Err(e) = self.cache.as_ref().borrow_mut().add_trade(trade) {
            log::error!("Error on cache insert: {e}");
        }

        // TODO: Handle synthetics

        let mut msgbus = self.msgbus.borrow_mut();
        let topic = msgbus.switchboard.get_trade_topic(trade.instrument_id);
        msgbus.publish(&topic, &trade as &dyn Any); // TODO: Optimize
    }

    fn handle_bar(&mut self, bar: Bar) {
        // TODO: Handle additional bar logic
        if self.config.validate_data_sequence {
            if let Some(last_bar) = self.cache.as_ref().borrow().bar(&bar.bar_type) {
                if bar.ts_event < last_bar.ts_event {
                    log::warn!(
                        "Bar {bar} was prior to last bar `ts_event` {}",
                        last_bar.ts_event
                    );
                    return; // `bar` is out of sequence
                }
                if bar.ts_init < last_bar.ts_init {
                    log::warn!(
                        "Bar {bar} was prior to last bar `ts_init` {}",
                        last_bar.ts_init
                    );
                    return; // `bar` is out of sequence
                }
                // TODO: Implement `bar.is_revision` logic
            }
        }

        if let Err(e) = self.cache.as_ref().borrow_mut().add_bar(bar) {
            log::error!("Error on cache insert: {e}");
        }

        let mut msgbus = self.msgbus.borrow_mut();
        let topic = msgbus.switchboard.get_bar_topic(bar.bar_type);
        msgbus.publish(&topic, &bar as &dyn Any); // TODO: Optimize
    }

    // -- RESPONSE HANDLERS -----------------------------------------------------------------------

    fn handle_instruments(&self, instruments: Arc<Vec<InstrumentAny>>) {
        // TODO improve by adding bulk update methods to cache and database
        let mut cache = self.cache.as_ref().borrow_mut();
        for instrument in instruments.iter() {
            if let Err(e) = cache.add_instrument(instrument.clone()) {
                log::error!("Error on cache insert: {e}");
            }
        }
    }

    fn handle_quotes(&self, quotes: Arc<Vec<QuoteTick>>) {
        if let Err(e) = self.cache.as_ref().borrow_mut().add_quotes(&quotes) {
            log::error!("Error on cache insert: {e}");
        }
    }

    fn handle_trades(&self, trades: Arc<Vec<TradeTick>>) {
        if let Err(e) = self.cache.as_ref().borrow_mut().add_trades(&trades) {
            log::error!("Error on cache insert: {e}");
        }
    }

    fn handle_bars(&self, bars: Arc<Vec<Bar>>) {
        if let Err(e) = self.cache.as_ref().borrow_mut().add_bars(&bars) {
            log::error!("Error on cache insert: {e}");
        }
    }

    // -- INTERNAL --------------------------------------------------------------------------------

    fn update_order_book(&self, data: &Data) {
        // Only apply data if there is a book being managed,
        // as it may be being managed manually.
        if let Some(book) = self
            .cache
            .as_ref()
            .borrow_mut()
            .order_book(data.instrument_id())
        {
            match data {
                Data::Delta(delta) => book.apply_delta(delta),
                Data::Deltas(deltas) => book.apply_deltas(deltas),
                Data::Depth10(depth) => book.apply_depth(depth),
                _ => log::error!("Invalid data type for book update"),
            }
        }
    }
}

pub struct SubscriptionCommandHandler {
    id: Ustr,
    data_engine: Rc<RefCell<DataEngine>>,
}

impl MessageHandler for SubscriptionCommandHandler {
    fn id(&self) -> Ustr {
        self.id
    }

    fn handle(&self, message: &dyn Any) {
        self.data_engine.borrow_mut().execute(message);
    }
    fn handle_response(&self, _resp: DataResponse) {}
    fn handle_data(&self, _resp: Data) {}
    fn as_any(&self) -> &dyn Any {
        self
    }
}

////////////////////////////////////////////////////////////////////////////////
// Tests
////////////////////////////////////////////////////////////////////////////////
#[cfg(test)]
mod tests {
    use indexmap::indexmap;
    use nautilus_common::{
        clock::TestClock,
        messages::data::Action,
        msgbus::{
            handler::ShareableMessageHandler,
            stubs::{get_call_check_shareable_handler, CallCheckMessageHandler},
            switchboard::MessagingSwitchboard,
        },
    };
    use nautilus_core::{nanos::UnixNanos, uuid::UUID4};
    use nautilus_model::{
        enums::BookType,
        identifiers::TraderId,
        instruments::{currency_pair::CurrencyPair, stubs::audusd_sim},
    };
    use rstest::*;

    use super::*;
    use crate::mocks::MockDataClient;

    #[fixture]
    fn trader_id() -> TraderId {
        TraderId::default()
    }

    #[fixture]
    fn client_id() -> ClientId {
        ClientId::default()
    }

    #[fixture]
    fn venue() -> Venue {
        Venue::default()
    }

    #[fixture]
    fn clock() -> Box<TestClock> {
        Box::new(TestClock::new())
    }

    #[fixture]
    fn cache() -> Rc<RefCell<Cache>> {
        Rc::new(RefCell::new(Cache::default()))
    }

    #[fixture]
    fn msgbus(trader_id: TraderId) -> Rc<RefCell<MessageBus>> {
        Rc::new(RefCell::new(MessageBus::new(
            trader_id,
            UUID4::new(),
            None,
            None,
        )))
    }

    #[fixture]
    fn switchboard(msgbus: Rc<RefCell<MessageBus>>) -> MessagingSwitchboard {
        msgbus.borrow().switchboard.clone()
    }

    #[fixture]
    fn data_engine(
        clock: Box<TestClock>,
        cache: Rc<RefCell<Cache>>,
        msgbus: Rc<RefCell<MessageBus>>,
    ) -> Rc<RefCell<DataEngine>> {
        let data_engine = DataEngine::new(clock, cache, msgbus, None);
        Rc::new(RefCell::new(data_engine))
    }

    // TODO: For some reason using this fixture causes the tests to fail?
    #[fixture]
    fn subscription_handler(
        data_engine: Rc<RefCell<DataEngine>>,
        switchboard: MessagingSwitchboard,
    ) -> ShareableMessageHandler {
        ShareableMessageHandler(Rc::new(SubscriptionCommandHandler {
            id: switchboard.data_engine_execute,
            data_engine,
        }))
    }

    #[fixture]
    fn data_client(
        client_id: ClientId,
        venue: Venue,
        cache: Rc<RefCell<Cache>>,
        msgbus: Rc<RefCell<MessageBus>>,
        clock: Box<TestClock>,
    ) -> DataClientAdapter {
        let client = Box::new(MockDataClient::new(cache, msgbus, client_id, venue));
        DataClientAdapter::new(client_id, venue, client, clock)
    }

    #[rstest]
    fn test_execute_subscribe_custom_data(
        audusd_sim: CurrencyPair,
        msgbus: Rc<RefCell<MessageBus>>,
        switchboard: MessagingSwitchboard,
        data_engine: Rc<RefCell<DataEngine>>,
        data_client: DataClientAdapter,
    ) {
        let client_id = data_client.client_id;
        let venue = data_client.venue;
        data_engine.borrow_mut().register_client(data_client, None);

        let data_type = DataType::new(stringify!(String), None);
        let cmd = SubscriptionCommand::new(
            client_id,
            venue,
            data_type.clone(),
            Action::Subscribe,
            UUID4::new(),
            UnixNanos::default(),
        );

        let endpoint = switchboard.data_engine_execute;
        let handler = ShareableMessageHandler(Rc::new(SubscriptionCommandHandler {
            id: endpoint,
            data_engine: data_engine.clone(),
        }));
        msgbus.borrow_mut().register(endpoint, handler);
        msgbus.borrow().send(&endpoint, &cmd as &dyn Any);

        assert!(data_engine
            .borrow()
            .subscribed_custom_data()
            .contains(&data_type));
    }

    #[rstest]
    fn test_execute_subscribe_order_book_deltas(
        audusd_sim: CurrencyPair,
        msgbus: Rc<RefCell<MessageBus>>,
        switchboard: MessagingSwitchboard,
        data_engine: Rc<RefCell<DataEngine>>,
        data_client: DataClientAdapter,
    ) {
        let client_id = data_client.client_id;
        let venue = data_client.venue;
        data_engine.borrow_mut().register_client(data_client, None);

        let metadata = indexmap! {
            "instrument_id".to_string() => audusd_sim.id.to_string(),
            "book_type".to_string() => BookType::L3_MBO.to_string(),
        };
        let data_type = DataType::new(stringify!(OrderBookDelta), Some(metadata));
        let cmd = SubscriptionCommand::new(
            client_id,
            venue,
            data_type,
            Action::Subscribe,
            UUID4::new(),
            UnixNanos::default(),
        );

        let endpoint = switchboard.data_engine_execute;
        let handler = ShareableMessageHandler(Rc::new(SubscriptionCommandHandler {
            id: endpoint,
            data_engine: data_engine.clone(),
        }));
        msgbus.borrow_mut().register(endpoint, handler);
        msgbus.borrow().send(&endpoint, &cmd as &dyn Any);

        assert!(data_engine
            .borrow()
            .subscribed_order_book_deltas()
            .contains(&audusd_sim.id));
    }

    #[rstest]
    fn test_execute_subscribe_order_book_snapshots(
        audusd_sim: CurrencyPair,
        msgbus: Rc<RefCell<MessageBus>>,
        switchboard: MessagingSwitchboard,
        data_engine: Rc<RefCell<DataEngine>>,
        data_client: DataClientAdapter,
    ) {
        let client_id = data_client.client_id;
        let venue = data_client.venue;
        data_engine.borrow_mut().register_client(data_client, None);

        let metadata = indexmap! {
            "instrument_id".to_string() => audusd_sim.id.to_string(),
            "book_type".to_string() => BookType::L2_MBP.to_string(),
        };
        let data_type = DataType::new(stringify!(OrderBookDeltas), Some(metadata));
        let cmd = SubscriptionCommand::new(
            client_id,
            venue,
            data_type,
            Action::Subscribe,
            UUID4::new(),
            UnixNanos::default(),
        );

        let endpoint = switchboard.data_engine_execute;
        let handler = ShareableMessageHandler(Rc::new(SubscriptionCommandHandler {
            id: endpoint,
            data_engine: data_engine.clone(),
        }));
        msgbus.borrow_mut().register(endpoint, handler);
        msgbus.borrow().send(&endpoint, &cmd as &dyn Any);

        assert!(data_engine
            .borrow()
            .subscribed_order_book_snapshots()
            .contains(&audusd_sim.id));
    }

    #[rstest]
    fn test_execute_subscribe_instrument(
        audusd_sim: CurrencyPair,
        msgbus: Rc<RefCell<MessageBus>>,
        switchboard: MessagingSwitchboard,
        data_engine: Rc<RefCell<DataEngine>>,
        data_client: DataClientAdapter,
    ) {
        let client_id = data_client.client_id;
        let venue = data_client.venue;
        data_engine.borrow_mut().register_client(data_client, None);

        let metadata = indexmap! {
            "instrument_id".to_string() => audusd_sim.id.to_string(),
        };
        let data_type = DataType::new(stringify!(InstrumentAny), Some(metadata));
        let cmd = SubscriptionCommand::new(
            client_id,
            venue,
            data_type,
            Action::Subscribe,
            UUID4::new(),
            UnixNanos::default(),
        );

        let endpoint = switchboard.data_engine_execute;
        let handler = ShareableMessageHandler(Rc::new(SubscriptionCommandHandler {
            id: endpoint,
            data_engine: data_engine.clone(),
        }));
        msgbus.borrow_mut().register(endpoint, handler);
        msgbus.borrow().send(&endpoint, &cmd as &dyn Any);

        assert!(data_engine
            .borrow()
            .subscribed_instruments()
            .contains(&audusd_sim.id));
    }

    #[rstest]
    fn test_execute_subscribe_quote_ticks(
        audusd_sim: CurrencyPair,
        msgbus: Rc<RefCell<MessageBus>>,
        switchboard: MessagingSwitchboard,
        data_engine: Rc<RefCell<DataEngine>>,
        data_client: DataClientAdapter,
    ) {
        let client_id = data_client.client_id;
        let venue = data_client.venue;
        data_engine.borrow_mut().register_client(data_client, None);

        let metadata = indexmap! {
            "instrument_id".to_string() => audusd_sim.id.to_string(),
        };
        let data_type = DataType::new(stringify!(QuoteTick), Some(metadata));
        let cmd = SubscriptionCommand::new(
            client_id,
            venue,
            data_type,
            Action::Subscribe,
            UUID4::new(),
            UnixNanos::default(),
        );

        let endpoint = switchboard.data_engine_execute;
        let handler = ShareableMessageHandler(Rc::new(SubscriptionCommandHandler {
            id: endpoint,
            data_engine: data_engine.clone(),
        }));
        msgbus.borrow_mut().register(endpoint, handler);
        msgbus.borrow().send(&endpoint, &cmd as &dyn Any);

        assert!(data_engine
            .borrow()
            .subscribed_quote_ticks()
            .contains(&audusd_sim.id));
    }

    #[rstest]
    fn test_execute_subscribe_trade_ticks(
        audusd_sim: CurrencyPair,
        msgbus: Rc<RefCell<MessageBus>>,
        switchboard: MessagingSwitchboard,
        data_engine: Rc<RefCell<DataEngine>>,
        data_client: DataClientAdapter,
    ) {
        let client_id = data_client.client_id;
        let venue = data_client.venue;
        data_engine.borrow_mut().register_client(data_client, None);

        let metadata = indexmap! {
            "instrument_id".to_string() => audusd_sim.id.to_string(),
        };
        let data_type = DataType::new(stringify!(TradeTick), Some(metadata));
        let cmd = SubscriptionCommand::new(
            client_id,
            venue,
            data_type,
            Action::Subscribe,
            UUID4::new(),
            UnixNanos::default(),
        );

        let endpoint = switchboard.data_engine_execute;
        let handler = ShareableMessageHandler(Rc::new(SubscriptionCommandHandler {
            id: endpoint,
            data_engine: data_engine.clone(),
        }));
        msgbus.borrow_mut().register(endpoint, handler);
        msgbus.borrow().send(&endpoint, &cmd as &dyn Any);

        assert!(data_engine
            .borrow()
            .subscribed_trade_ticks()
            .contains(&audusd_sim.id));
    }

    #[rstest]
    fn test_execute_subscribe_bars(
        audusd_sim: CurrencyPair,
        msgbus: Rc<RefCell<MessageBus>>,
        switchboard: MessagingSwitchboard,
        data_engine: Rc<RefCell<DataEngine>>,
        data_client: DataClientAdapter,
    ) {
        let client_id = data_client.client_id;
        let venue = data_client.venue;
        data_engine.borrow_mut().register_client(data_client, None);

        let bar_type = BarType::from("AUDUSD.SIM-1-MINUTE-LAST-INTERNAL");
        let metadata = indexmap! {
            "bar_type".to_string() => bar_type.to_string(),
        };
        let data_type = DataType::new(stringify!(Bar), Some(metadata));
        let cmd = SubscriptionCommand::new(
            client_id,
            venue,
            data_type,
            Action::Subscribe,
            UUID4::new(),
            UnixNanos::default(),
        );

        let endpoint = switchboard.data_engine_execute;
        let handler = ShareableMessageHandler(Rc::new(SubscriptionCommandHandler {
            id: endpoint,
            data_engine: data_engine.clone(),
        }));
        msgbus.borrow_mut().register(endpoint, handler);
        msgbus.borrow().send(&endpoint, &cmd as &dyn Any);

        assert!(data_engine.borrow().subscribed_bars().contains(&bar_type));
    }

    #[rstest]
    fn test_process_quote_tick(
        audusd_sim: CurrencyPair,
        msgbus: Rc<RefCell<MessageBus>>,
        switchboard: MessagingSwitchboard,
        data_engine: Rc<RefCell<DataEngine>>,
        data_client: DataClientAdapter,
    ) {
        let client_id = data_client.client_id;
        let venue = data_client.venue;
        data_engine.borrow_mut().register_client(data_client, None);

        let metadata = indexmap! {
            "instrument_id".to_string() => audusd_sim.id.to_string(),
        };
        let data_type = DataType::new(stringify!(QuoteTick), Some(metadata));
        let cmd = SubscriptionCommand::new(
            client_id,
            venue,
            data_type,
            Action::Subscribe,
            UUID4::new(),
            UnixNanos::default(),
        );

        let endpoint = switchboard.data_engine_execute;
        let handler = ShareableMessageHandler(Rc::new(SubscriptionCommandHandler {
            id: endpoint,
            data_engine: data_engine.clone(),
        }));
        msgbus.borrow_mut().register(endpoint, handler);
        msgbus.borrow().send(&endpoint, &cmd as &dyn Any);

        let quote = QuoteTick::default();

        let mut msgbus = msgbus.borrow_mut();
        let topic = msgbus.switchboard.get_quote_topic(quote.instrument_id);
        let handler = get_call_check_shareable_handler(Ustr::from("quote-handler"));
        msgbus.subscribe(topic, handler.clone(), None);

        data_engine.borrow_mut().process(Data::Quote(quote));
        assert!(!handler
            .0
            .as_ref()
            .as_any()
            .downcast_ref::<CallCheckMessageHandler>()
            .unwrap()
            .was_called());
    }

    #[rstest]
    fn test_process_trade_tick(
        audusd_sim: CurrencyPair,
        msgbus: Rc<RefCell<MessageBus>>,
        switchboard: MessagingSwitchboard,
        data_engine: Rc<RefCell<DataEngine>>,
        data_client: DataClientAdapter,
    ) {
        let client_id = data_client.client_id;
        let venue = data_client.venue;
        data_engine.borrow_mut().register_client(data_client, None);

        let metadata = indexmap! {
            "instrument_id".to_string() => audusd_sim.id.to_string(),
        };
        let data_type = DataType::new(stringify!(QuoteTick), Some(metadata));
        let cmd = SubscriptionCommand::new(
            client_id,
            venue,
            data_type,
            Action::Subscribe,
            UUID4::new(),
            UnixNanos::default(),
        );

        let endpoint = switchboard.data_engine_execute;
        let handler = ShareableMessageHandler(Rc::new(SubscriptionCommandHandler {
            id: endpoint,
            data_engine: data_engine.clone(),
        }));
        msgbus.borrow_mut().register(endpoint, handler);
        msgbus.borrow().send(&endpoint, &cmd as &dyn Any);

        let trade = TradeTick::default();

        let mut msgbus = msgbus.borrow_mut();
        let topic = msgbus.switchboard.get_quote_topic(trade.instrument_id);
        let handler = get_call_check_shareable_handler(Ustr::from("trade-handler"));
        msgbus.subscribe(topic, handler.clone(), None);

        data_engine.borrow_mut().process(Data::Trade(trade));
        assert!(!handler
            .0
            .as_ref()
            .as_any()
            .downcast_ref::<CallCheckMessageHandler>()
            .unwrap()
            .was_called());
    }
}