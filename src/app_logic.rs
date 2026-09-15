// SPDX-License-Identifier: GPL-3.0-only

//! This module contains the main application code.

use slint::language::StandardListViewItem;
use std::{
    thread,
    time::Duration,
    fs::{OpenOptions, self},
    sync::{mpsc, Mutex, LazyLock, OnceLock, RwLock},
    collections::VecDeque,
    io::{Cursor, Write},
    fmt::Display,
    panic,
};
use chrono::{Local, DateTime, TimeDelta};
use serde_json::Value;
use reqwest::blocking;
use crate::{FILE_PREFIX, SAFELY_PRUNE, cities::CITIES};
use backtrace::Backtrace;

slint::include_modules!();

/// This is the contents of the file "adhan.ogg" as a constant slice.
const ADHAN: &[u8] = include_bytes!("../adhan.ogg").as_slice();
/// This is the contents of the file "fajr-adhan.ogg" as a constant slice.
const FAJR_ADHAN: &[u8] = include_bytes!("../fajr-adhan.ogg").as_slice();
/// Half a second; this is a constant that I used multiple times in the code.
const SLEEP_TIME: Duration = Duration::from_millis(500);

/// This is a Mutex used as a lock to ensure that file write operations happen
/// sequentially.
static LOG_LOCK: Mutex<()> = Mutex::new(());
/// This contains the list of adhan timings for the current and upcoming year.
static TIMINGS: Mutex<VecDeque<Timing>> = Mutex::new(VecDeque::new());
/// This contains the current country as set by the user; it defaults to the
/// United States.
static COUNTRY: Mutex<String> = Mutex::new(String::new());
/// This is a list of the countries in the map, lowercased. Since this is
/// constant, it is stored in a LazyLock instead of a Mutex. It is used for
/// real-time searching through the data.
static COUNTRIES: LazyLock<Vec<String>> = LazyLock::new(|| CITIES.keys().map(|x| x.to_lowercase()).collect());
/// This contains the current country as set by the user; it defaults to Dallas
/// (Texas).
static CITY: Mutex<String> = Mutex::new(String::new());
/// This is a list of the cities in the map within the current country,
/// lowercased. Since this changes based on the country, it is stored in a
/// Mutex. It is used for real-time searching through the data.
static COUNTRY_CITIES: Mutex<Vec<String>> = Mutex::new(Vec::new());
/// This stores the school of thought as set by the user; it defaults to Hanafi
/// time.
static SCHOOL: Mutex<String> = Mutex::new(String::new());
/// This stores a weak reference to the app window (see
/// [`slint::Weak`](https://docs.rs/slint/latest/slint/struct.Weak.html)). It is
/// in a Mutex because Android sometimes restarts the window, which then
/// necessitates a new weak reference.
static WEAKAPP: LazyLock<RwLock<slint::Weak<Clock>>> = LazyLock::new(|| RwLock::new(Default::default()));
/// This stores whether or not Android has restarted the window. It is used to
/// prevent restarting all the threads and reloading the data from scratch.
static RAN_BEFORE: Mutex<bool> = Mutex::new(false);
/// This is used not to update the data if the user has requested another update
/// after the current update, and that one went through first.
static DATA_LOCK: Mutex<u32> = Mutex::new(0);
/// This is used internally as a notification that the data has been reloaded
/// and the interface needs updating.
static UPDATE_TX: OnceLock<mpsc::Sender<()>> = OnceLock::new();

/// This is a tuple containing the name of the prayer, the time at which its
/// adhan should be called, and its position in a day. I did not make it a
/// struct for some reason and was too lazy to fix it afterwards.
type Timing = (String, DateTime<Local>, usize);

/// This function initializes the main window and all the threads if necessary.
pub fn main_window() -> Clock {
    panic::set_hook(Box::new(move |panic_info| {
        log(
            format!("ERROR: {}\n{:#?}", panic_info, Backtrace::new()),
        );
    }));
    let app = Clock::new().unwrap();
    *WEAKAPP.write().unwrap() = app.as_weak();
    app.on_city_picked(move |new_city| {
        thread::spawn(move || {
            *CITY.lock().unwrap() = new_city.clone().into();
            fs::write(format!("{}city.txt", FILE_PREFIX), new_city).unwrap();
            load_data();
        });
    });
    app.on_school_picked(move |new_school| {
        thread::spawn(move || {
            *SCHOOL.lock().unwrap() = new_school.clone().into();
            fs::write(format!("{}school.txt", FILE_PREFIX), new_school).unwrap();
            load_data();
        });
    });
    app.on_country_picked(move |new_country| {
        thread::spawn(move || {
            let mut country = COUNTRY.lock().unwrap();
            *country = new_country.clone().into();
            fs::write(format!("{}country.txt", FILE_PREFIX), new_country).unwrap();
            let cities = CITIES[&country].keys();
            drop(country);
            *COUNTRY_CITIES.lock().unwrap() = cities.clone().map(|x| x.to_lowercase()).collect();
            let cities: Vec<_> = cities.copied().collect();
            let mut city = CITY.lock().unwrap();
            *city = (*cities[0]).into();
            fs::write(format!("{}city.txt", FILE_PREFIX), (*city).clone()).unwrap();
            drop(city);
            let cities: Vec<StandardListViewItem> = cities.iter().map(|&x| x.into()).collect();
            ensure_run_in_event_loop(move |app| {
                app.set_cities(cities.as_slice().into());
                app.invoke_update_city(0);
            });
            load_data();
        });
    });
    let weakapp = app.as_weak();
    app.on_search_country(move |country_name| {
        if country_name.is_empty() { return; }
        let pos = match COUNTRIES.binary_search(&country_name.to_lowercase()) {
            Ok(x) => x as i32,
            Err(x) => x as i32,
        };
        weakapp.upgrade().unwrap().invoke_show_country(pos);
    });
    let weakapp = app.as_weak();
    app.on_search_city(move |city_name| {
        if city_name.is_empty() { return; }
        let pos = match COUNTRY_CITIES.lock().unwrap().binary_search(&city_name.to_lowercase()) {
            Ok(x) => x as i32,
            Err(x) => x as i32,
        };
        weakapp.upgrade().unwrap().invoke_show_city(pos);
    });
    let mut ran_before = RAN_BEFORE.lock().unwrap();
    if *ran_before {
        thread::spawn(|| log("INFO: Tried to run the app a second time."));
        thread::spawn(|| {
            init_city_country(false);
            let timings = TIMINGS.lock().unwrap();
            let mut current_times: Vec<_> = timings.range(0..6).cloned().collect();
            handle_prayer_change(&timings, &mut current_times);
            drop(timings);
        });
        app.set_school((*SCHOOL.lock().unwrap()).clone().into());
        return app;
    } else {
        thread::spawn(|| log("INFO: Started the application."));
        *ran_before = true;
    }
    let (update_tx, update_rx) = mpsc::channel();
    let _ = UPDATE_TX.set(update_tx);
    load_location_school();
    app.set_school((*SCHOOL.lock().unwrap()).clone().into());
    init_city_country(true);
    let (time_tx, time_rx) = mpsc::channel();
    let (adhan_tx, adhan_rx) = mpsc::channel();
    let (srise_tx, srise_rx) = mpsc::channel();
    thread::spawn(move || time_loop(time_tx));
    thread::spawn(move || update_loop(time_rx, update_rx, adhan_tx));
    thread::spawn(move || adhan_loop(adhan_rx, srise_tx));
    thread::spawn(move || srise_loop(srise_rx));
    if SAFELY_PRUNE {
        thread::spawn(prune);
    }
    return app;
}

/// On platforms where the app has an exclusive folder, this function allows the
/// app to delete old versions of data.
fn prune() {
    let pattern = format!("{}*[!-][!l][!m].json", FILE_PREFIX);
    for file in glob::glob(&pattern).unwrap() {
        let file = file.unwrap();
        if fs::remove_file(&file).is_ok() {
            log(format!("INFO: Removed {}.", file.display()));
        }
    }
}

/// This function handles the special case of sunrise; it tells the app that
/// sunrise time is over thirty minutes after sunrise.
fn srise_loop(srise_rx: mpsc::Receiver<()>) {
    loop {
        srise_rx.recv().unwrap();
        thread::sleep(Duration::from_mins(15));
        ensure_run_in_event_loop(move |app| {
            app.set_adhan_playing(false);
        });
    }
}

/// This function runs a loop that handles playing the adhan whenever it's time.
fn adhan_loop(adhan_rx: mpsc::Receiver<String>, srise_tx: mpsc::Sender<()>) {
    loop {
        log("INFO: Adhan loop advanced");
        let adhan = adhan_rx.recv().unwrap();
        log(format!("INFO: {} time", adhan));
        if adhan == "Sunrise" {
            srise_tx.send(()).unwrap();
            continue;
        }
        let sound = if adhan == "Fajr" {FAJR_ADHAN} else {ADHAN};
        let sink_handle = rodio::DeviceSinkBuilder::open_default_sink()
            .unwrap();
        log("INFO: Sound started");
        rodio::play(
            sink_handle.mixer(),
            Cursor::new(sound),
        ).unwrap().sleep_until_end();
        ensure_run_in_event_loop(move |app| {
            app.set_adhan_playing(false);
        });
        log("INFO: Sound ended");
    }
}

/// This function runs a loop that handles most of the graphical updates.
fn update_loop(
    time_rx: mpsc::Receiver<DateTime<Local>>,
    update_rx: mpsc::Receiver<()>,
    adhan_tx: mpsc::Sender<String>,
) {
    load_data();
    let timings = TIMINGS.lock().unwrap();
    let mut current_times: Vec<_> = timings.range(0..6).cloned().collect();
    handle_prayer_change(&timings, &mut current_times);
    drop(timings);
    let mut tz = Local::now().format("%z").to_string();
    loop {
        let time = time_rx.recv().unwrap();
        let mut timings = TIMINGS.lock().unwrap();
        let new_tz = time.format("%z").to_string();
        if tz != new_tz {
            drop(timings);
            load_data();
            timings = TIMINGS.lock().unwrap();
            handle_prayer_change(&timings, &mut current_times);
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
        let time_fmt = time.format("%I:%M:%S %p").to_string();
        let date = time.format("%a %m-%d-%Y").to_string();
        ensure_run_in_event_loop(move |app| {
            app.set_next_prayer(next_prayer.into());
            app.set_time(time_fmt.into());
            app.set_date(date.into());
        });
        if update_rx.try_recv().is_ok() {
            handle_prayer_change(&timings, &mut current_times);
            log("INFO: Updated because new data was loaded.");
        }
        if time >= timings[0].1 {
            if (time - timings[0].1).num_minutes() > 1 {
                while timings[0].1 < time {
                    timings.pop_front().unwrap();
                }
                handle_prayer_change(&timings, &mut current_times);
                continue;
            }
            let adhan = timings.pop_front().unwrap().0;
            log(format!("INFO: {} sent", adhan));
            adhan_tx.send(adhan).unwrap();
            handle_prayer_change(&timings, &mut current_times);
            ensure_run_in_event_loop(move |app| {
                app.set_adhan_playing(true);
            });
            if timings.len() <= 100 {
                drop(timings);
                load_data();
                timings = TIMINGS.lock().unwrap();
            }
        }
    }
}

/// This function handles reading the time every half second and, if it has
/// changed, instructing the graphical updates loop to update.
fn time_loop(time_tx: mpsc::Sender<DateTime<Local>>) {
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
        prev_time = current_time;
        time_tx.send(current_time).unwrap();
        thread::sleep(SLEEP_TIME);
    }
}

/// This function provides the data for the current and next year as [`Value`]s.
fn get_data() -> (Value, Value) {
    let year: i32 = format!("{}", Local::now().format("%Y")).parse().unwrap();
    let country = COUNTRY.lock().unwrap();
    let city = CITY.lock().unwrap();
    let school = match SCHOOL.lock().unwrap().as_str() {
        "Majority time" => 0,
        "Hanafi time" => 1,
        _ => panic!("The school should be either Majority time or Hanafi time"),
    };
    return (
        get_year_data(year, &country, &city, school),
        get_year_data(year + 1, &country, &city, school),
    );
}

/// This function, given the country, city, school of thought, and year, looks
/// for the cached adhan data and downloads it if it is missing. It then returns
/// the data as a [`Value`].
fn get_year_data(year: i32, country: &String, city: &String, school: i32) -> Value {
    let file = format!(
        "{}{}-{}-{}-{}-lm.json",
        FILE_PREFIX,
        year,
        country,
        city,
        school,
    );
    let data = if fs::exists(&file).unwrap() {
        fs::read_to_string(&file).unwrap()
    } else {
        let tmp = blocking::get(&format!(
            "https://api.aladhan.com/v1/calendar/{}?{}&school={}",
            year,
            CITIES[country][city],
            school,
        )).unwrap().text().unwrap();
        fs::write(&file, tmp.clone()).unwrap();
        tmp
    };
    return serde_json::from_str::<Value>(&data).unwrap()["data"].take();
}

/// This function loads the adhan data into the [`TIMINGS`] static.
fn load_data() {
    let mut data_lock = DATA_LOCK.lock().unwrap();
    *data_lock += 1;
    let data_id = *data_lock;
    drop(data_lock);
    let today = Local::now();
    let tz = today.format("%z").to_string();
    let (this_year, next_year) = get_data();
    let mut this_year_parsed = Vec::new();
    let mut next_year_parsed = Vec::new();
    for month in 1..13 {
        let month = month.to_string();
        this_year_parsed.extend(this_year[&month].as_array().unwrap());
        next_year_parsed.extend(next_year[&month].as_array().unwrap());
    }
    this_year_parsed.extend(next_year_parsed);
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
    let data_lock = DATA_LOCK.lock().unwrap();
    if *data_lock == data_id {
        let mut timings = TIMINGS.lock().unwrap();
        timings.clear();
        timings.extend(parsed_data);
        UPDATE_TX.wait().send(()).unwrap();
    } else {
        log("INFO: Did not update data because of lock.");
    }
}

/// This function is used for logging.
fn log(message: impl Display) {
    let _lock = LOG_LOCK.lock();
    let mut file;
    match OpenOptions::new().append(true).create(true).open(
        format!("{}log.txt", FILE_PREFIX)
    ) {
        Ok(x) => file = x,
        Err(_) => return,
    };
    let _ = writeln!(file, "{}: {}", Local::now(), message);
}

/// This function initializes the city, country, and school of thought, loading
/// the data from files or just setting defaults.
fn load_location_school() {
    let to_load = [
        ("city", "Dallas (Texas)", &CITY),
        ("country", "United States", &COUNTRY),
        ("school", "Hanafi time", &SCHOOL),
    ];
    for item in to_load {
        let file = format!("{}{}.txt", FILE_PREFIX, item.0);
        let mut value = item.1.into();
        if fs::exists(&file).unwrap() {
            match fs::read_to_string(&file) {
                Ok(x) => value = x,
                Err(e) => log(format!("Could not read the {} file because of {:#?}", item.0, e)),
            }
        }
        *item.2.lock().unwrap() = value;
    }
}

/// This function initializes the city and country data and synchronizes the
/// window.
fn init_city_country(main_thread: bool) {
    let countries: Vec<_> = CITIES.keys().copied().collect();
    let country = COUNTRY.lock().unwrap();
    let country_pos = countries.binary_search(&country.as_str()).unwrap();
    let cities = CITIES[&*country].keys();
    *COUNTRY_CITIES.lock().unwrap() = cities.clone().map(|x| x.to_lowercase()).collect();
    let cities: Vec<_> = cities.copied().collect();
    let city = CITY.lock().unwrap();
    let city_pos = cities.binary_search(&city.as_str()).unwrap();
    let cities: Vec<StandardListViewItem> = cities.iter().map(|&x| x.into()).collect();
    let countries: Vec<StandardListViewItem> = countries.iter().map(|&x| x.into()).collect();
    let func = move |app: Clock| {
        app.set_city(city_pos as i32);
        app.set_country(country_pos as i32);
        app.set_cities(cities.as_slice().into());
        app.set_countries(countries.as_slice().into());
    };
    if main_thread {
        let app = WEAKAPP.read().unwrap().clone();
        func(app.unwrap());
    } else {
        ensure_run_in_event_loop(func);
    }
}

/// This function is run whenever a change occurs in the list of prayer times
/// that needs a graphical update.
fn handle_prayer_change(
    timings: &VecDeque<Timing>,
    current_times: &mut [Timing],
) {
    for time in timings.range(0..6) {
        current_times[time.2] = time.clone();
    }
    let current_prayer = timings[5].0.clone();
    let prayer_times: Vec<_> = current_times.iter().map(|x| PrayerTime{
        prayer: x.0.clone().into(),
        time: x.1.format("%I:%M").to_string().into(),
        ampm: x.1.format("%p").to_string().into(),
    }).collect();
    ensure_run_in_event_loop(move |app| {
        app.invoke_update_prayer_times(prayer_times.as_slice().into());
        app.invoke_update_current_prayer(current_prayer.into());
    });
}

/// This function requests that the provided function be run in the event loop
/// and blocks until it completes.
fn ensure_run_in_event_loop<F>(func: F)
where F: FnOnce(Clock) + Send + Clone + 'static {
    let (result_tx, result_rx) = mpsc::channel();
    loop {
        let func = func.clone();
        let result_tx = result_tx.clone();
        slint::invoke_from_event_loop(move || {
            let app = WEAKAPP.read().unwrap().clone();
            let app = match app.upgrade() {
                Some(x) => x,
                None => {
                    result_tx.send(false).unwrap();
                    return;
                }
            };
            func(app);
            result_tx.send(true).unwrap();
        }).unwrap();
        if result_rx.recv().unwrap() {
            return;
        } else {
            thread::sleep(SLEEP_TIME);
        }
    }
}
