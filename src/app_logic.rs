use slint;
use std::{
    thread,
    time::Duration,
    fs,
    fs::OpenOptions,
    sync::mpsc,
    collections::VecDeque,
    io::{Cursor, Write},
    fmt::Display,
};
use chrono::{Local, DateTime, TimeDelta};
use serde_json::Value;
use reqwest::blocking;
use rodio;

slint::include_modules!();

const ADHAN: &[u8] = include_bytes!("../adhan.ogg").as_slice();
const FAJR_ADHAN: &[u8] = include_bytes!("../fajr-adhan.ogg").as_slice();
const SLEEP_TIME: Duration = Duration::from_millis(500);
const FIFTEEN_MINS: Duration = Duration::from_mins(15);

trait LogError<T: Display, E> {
    fn unwrap_or_log(self, file_prefix: &'static str) -> T;
}

impl LogError for Result<T, E> {
    fn unwrap_or_log(self, file_prefix: &'static str) -> T {
        match self {
            Ok(x) => return x,
            Err(e) => {
                log(file_prefix, e);
                panic!();
            },
        }
    }
}

pub fn main_window(file_prefix: &'static str) -> Clock {
    let app = Clock::new().unwrap();
    let weakapp = app.as_weak();
    let (time_tx, time_rx) = mpsc::channel();
    let (adhan_tx, adhan_rx) = mpsc::channel();
    let (srise_tx, srise_rx) = mpsc::channel();
    thread::spawn(move || {
        let mut prev_time = Local::now();
        loop {
            let current_time = Local::now();
            let current_time = current_time - TimeDelta::nanoseconds(
                current_time.timestamp_subsec_nanos().into()
            );
            if current_time == prev_time {
                thread::sleep(SLEEP_TIME);
                continue;
            }
            prev_time = current_time.clone();
            time_tx.send(current_time).unwrap();
            let time = current_time.format("%I:%M:%S %p").to_string();
            let date = current_time.format("%a %m-%d-%Y").to_string();
            let app = weakapp.clone();
            slint::invoke_from_event_loop(move || {
                let app = app.unwrap();
                app.set_time(time.into());
                app.set_date(date.into());
            }).unwrap();
            thread::sleep(SLEEP_TIME);
        }
    });
    let weakapp = app.as_weak();
    thread::spawn(move || {
        let mut timings = load_data(file_prefix);
        let current_prayer = timings[5].0.clone();
        let mut current_times: Vec<_> = timings.range(0..6).map(|x| x.clone()).collect();
        for time in timings.range(0..6) {
            current_times[time.2] = time.clone();
        }
        let prayer_times: Vec<_> = current_times.iter().map(|x| PrayerTime{
            prayer: x.0.clone().into(),
            time: x.1.format("%I:%M").to_string().into(),
            ampm: x.1.format("%p").to_string().into(),
        }).collect();
        let app = weakapp.clone();
        slint::invoke_from_event_loop(move || {
            let app = app.unwrap();
            app.set_prayer_times(prayer_times.as_slice().into());
            app.set_current_prayer(current_prayer.into());
        }).unwrap();
        let mut tz = Local::now().format("%z").to_string();
        loop {
            let time = time_rx.recv().unwrap();
            let new_tz = time.format("%z").to_string();
            if tz != new_tz {
                timings = load_data(file_prefix);
                let current_prayer = timings[5].0.clone();
                for time in timings.range(0..6) {
                    current_times[time.2] = time.clone();
                }
                let prayer_times: Vec<_> = current_times.iter().map(|x| PrayerTime{
                    prayer: x.0.clone().into(),
                    time: x.1.format("%I:%M").to_string().into(),
                    ampm: x.1.format("%p").to_string().into(),
                }).collect();
                let app = weakapp.clone();
                slint::invoke_from_event_loop(move || {
                    let app = app.unwrap();
                    app.set_prayer_times(prayer_times.as_slice().into());
                    app.set_current_prayer(current_prayer.into());
                }).unwrap();
                tz = new_tz;
            }
            let next_prayer = &timings[0];
            let time_left = next_prayer.1 - time;
            let time_left = format!(
                "{:0>2}:{:0>2}:{:0>2}",
                time_left.num_hours(),
                time_left.num_minutes() % 60,
                time_left.num_seconds() % 60,
            );
            let next_prayer = format!("{} in\n{}", next_prayer.0, time_left);
            let app = weakapp.clone();
            slint::invoke_from_event_loop(move || {
                let app = app.unwrap();
                app.set_next_prayer(next_prayer.into());
            }).unwrap();
            if time >= timings[0].1 {
                let adhan = timings.pop_front().unwrap().0;
                log(file_prefix, format!("INFO: {} sent", adhan));
                adhan_tx.send(adhan).unwrap();
                let current_prayer = timings[5].0.clone();
                for time in timings.range(0..6) {
                    current_times[time.2] = time.clone();
                }
                let prayer_times: Vec<_> = current_times.iter().map(|x| PrayerTime{
                    prayer: x.0.clone().into(),
                    time: x.1.format("%I:%M").to_string().into(),
                    ampm: x.1.format("%p").to_string().into(),
                }).collect();
                let app = weakapp.clone();
                slint::invoke_from_event_loop(move || {
                    let app = app.unwrap();
                    app.set_prayer_times(prayer_times.as_slice().into());
                    app.set_current_prayer(current_prayer.into());
                    app.set_adhan_playing(true.into());
                }).unwrap();
                if timings.len() <= 100 {
                    timings = load_data(file_prefix);
                }
            }
        }
    });
    let weakapp = app.as_weak();
    thread::spawn(move || {
        loop {
            log(file_prefix, "INFO: Adhan loop advanced");
            let adhan = adhan_rx.recv().unwrap();
            log(file_prefix, format!("INFO: {} time", adhan));
            if adhan == "Sunrise" {
                srise_tx.send(true).unwrap();
                continue;
            }
            let sound = if adhan == "Fajr" {FAJR_ADHAN} else {ADHAN};
            let sink_handle;
            match rodio::DeviceSinkBuilder::open_default_sink() {
                Ok(x) => sink_handle = x,
                Err(e) => {
                    log(file_prefix, format!("ERROR: {:?}", e));
                    panic!();
                }
            }
            log(file_prefix, "INFO: Sound started");
            rodio::play(
                &sink_handle.mixer(),
                Cursor::new(sound),
            ).unwrap().sleep_until_end();
            let app = weakapp.clone();
            slint::invoke_from_event_loop(move || {
                let app = app.unwrap();
                app.set_adhan_playing(false.into());
            }).unwrap();
            log(file_prefix, "INFO: Sound ended");
        }
    });
    let weakapp = app.as_weak();
    thread::spawn(move || {
        loop {
            srise_rx.recv().unwrap();
            thread::sleep(FIFTEEN_MINS);
            let app = weakapp.clone();
            slint::invoke_from_event_loop(move || {
                let app = app.unwrap();
                app.set_adhan_playing(false.into());
            }).unwrap();
        }
    });
    return app;
}

fn get_data(file_prefix: &'static str) -> (String, String) {
    let mut year: u64 = format!("{}", Local::now().format("%Y")).parse().unwrap();
    let this_years_data;
    if fs::exists(format!("{}{}.json", file_prefix, year)).unwrap() {
        this_years_data = fs::read_to_string(
            format!("{}{}.json", file_prefix, year),
        ).unwrap();
    } else {
        this_years_data = blocking::get(&format!(
            "https://api.aladhan.com/v1/calendar/{year}?latitude=32.826708&longitude=-97.084055&method=2&school=1"
        )).unwrap().text().unwrap();
        fs::write(
            format!("{}{}.json", file_prefix, year),
            this_years_data.clone(),
        ).unwrap();
    }
    year += 1;
    let next_years_data;
    if fs::exists(format!("{}{}.json", file_prefix, year)).unwrap() {
        next_years_data = fs::read_to_string(
            format!("{}{}.json", file_prefix, year),
        ).unwrap();
    } else {
        next_years_data = blocking::get(&format!(
            "https://api.aladhan.com/v1/calendar/{year}?latitude=32.826708&longitude=-97.084055&method=2&school=1"
        )).unwrap().text().unwrap();
        fs::write(
            format!("{}{}.json", file_prefix, year),
            next_years_data.clone(),
        ).unwrap();
    }
    return (this_years_data, next_years_data);
}

fn load_data(file_prefix: &'static str) -> VecDeque<(String, DateTime<Local>, usize)> {
    let today = Local::now();
    let tz = today.format("%z").to_string();
    let (this_year, next_year) = get_data(file_prefix);
    let this_year = serde_json::from_str::<Value>(&this_year).unwrap()["data"].take();
    let next_year = serde_json::from_str::<Value>(&next_year).unwrap()["data"].take();
    let mut this_year_parsed = Vec::new();
    let mut next_year_parsed = Vec::new();
    for month in 1..13 {
        let month = month.to_string();
        this_year_parsed.extend(this_year[month.clone()].as_array().unwrap().as_slice());
        next_year_parsed.extend(next_year[month].as_array().unwrap().as_slice());
    }
    this_year_parsed.extend(next_year_parsed.as_slice());
    let mut parsed_data: VecDeque<_> = this_year_parsed.into_iter().flat_map(|day| {
        let timings = day["timings"].as_object().unwrap();
        let mut out = Vec::new();
        let prayers = [
            "Fajr",
            "Sunrise",
            "Dhuhr",
            "Asr",
            "Maghrib",
            "Isha",
        ];
        for (i, prayer) in prayers.iter().enumerate() {
            out.push((
                prayer.to_string(),
                DateTime::parse_from_str(
                    &format!(
                        "{} {} {}",
                        &timings[*prayer].to_string()[1..6],
                        &day["date"]["readable"].to_string()[1..12],
                        tz,
                    ),
                    "%H:%M %d %b %Y %z",
                ).unwrap().into(),
                i,
            ));
        }
        return out;
    }).collect();
    while parsed_data[0].1 < today {
        parsed_data.pop_front();
    }
    return parsed_data;
}

fn log(file_prefix: &'static str, message: impl Display) {
    let mut file;
    match OpenOptions::new().append(true).create(true).open(
        format!("{}log.txt", file_prefix)
    ) {
        Ok(x) => file = x,
        Err(_) => return,
    };
    let _ = writeln!(file, "{}: {}", Local::now(), message);
}
