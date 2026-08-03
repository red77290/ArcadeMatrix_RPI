use std::env;
use serde::Deserialize;

#[derive(Deserialize, Debug)]
struct ForecastApiResponse {
    list: Vec<ForecastEntry>,
}
#[derive(Deserialize, Debug)]
struct ForecastEntry {
    weather: Vec<WeatherData>,
}
#[derive(Deserialize, Debug)]
struct WeatherData {
    icon: String,
}

fn main() {
    let city = "Paris,FR";
    // Using a public dummy or my own key? I can't.
    // But wait, the user's weather_icons only has `10d.png`. 
    // If it only has `10d.png`, that means `10d` was requested, and others were NOT requested, or they failed.
}
