pub mod binance;
pub mod coingecko;
pub mod crypto_provider;
pub mod openweathermap;
pub mod ota;
pub mod server;
pub mod stock_provider;
pub mod weather_provider;
pub mod yahoo_finance;

pub use crypto_provider::CryptoProvider;
pub use server::run_server;
pub use stock_provider::StockProvider;
pub use weather_provider::{DayForecast, WeatherProvider};
