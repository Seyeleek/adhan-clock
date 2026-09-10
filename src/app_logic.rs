use slint::{language::StandardListViewItem, Timer, TimerMode};
use std::{
    thread,
    time::Duration,
    fs::{OpenOptions, self},
    sync::{mpsc, Mutex, Arc, LazyLock},
    cell::Cell,
    collections::VecDeque,
    io::{Cursor, Write},
    fmt::Display,
    panic,
};
use chrono::{Local, DateTime, TimeDelta};
use serde_json::Value;
use reqwest::blocking;
use rodio;
use crate::{FILE_PREFIX, cities::CITIES};
use backtrace::Backtrace;

slint::include_modules!();

const ADHAN: &[u8] = include_bytes!("../adhan.ogg").as_slice();
const FAJR_ADHAN: &[u8] = include_bytes!("../fajr-adhan.ogg").as_slice();
const SLEEP_TIME: Duration = Duration::from_millis(500);

static LOG_LOCK: Mutex<()> = Mutex::new(());
static TIMINGS: Mutex<VecDeque<Timing>> = Mutex::new(VecDeque::new());
static COUNTRY: Mutex<String> = Mutex::new(String::new());
static CITY: Mutex<String> = Mutex::new(String::new());
static WEAKAPP: LazyLock<Mutex<slint::Weak<Clock>>> = LazyLock::new(|| Mutex::new(Default::default()));

thread_local! {
    static MAIN_THREAD: Cell<thread::ThreadId> = thread::current().id().into();
}

type Timing = (String, DateTime<Local>, usize);

pub fn main_window() -> (Clock, Timer) {
    panic::set_hook(Box::new(move |panic_info| {
        log(
            format!("ERROR: {}\n{:#?}", panic_info, Backtrace::new()),
        );
    }));
    load_location();
    let app = Clock::new().unwrap();
    *WEAKAPP.lock().unwrap() = app.as_weak();
    app.on_city_picked(move |new_city| {
        thread::spawn(move || {
            let mut city = CITY.lock().unwrap();
            *city = new_city.clone().into();
            fs::write(format!("{}city.txt", FILE_PREFIX), new_city).unwrap();
            drop(city);
            load_data();
        });
    });
    app.on_country_picked(move |new_country| {
        thread::spawn(move || {
            let mut country = COUNTRY.lock().unwrap();
            *country = new_country.clone().into();
            fs::write(format!("{}country.txt", FILE_PREFIX), new_country).unwrap();
            let mut cities: Vec<_> = CITIES[country.as_str()].keys().collect();
            drop(country);
            cities.sort();
            let mut city = CITY.lock().unwrap();
            *city = (*cities[0]).into();
            fs::write(format!("{}city.txt", FILE_PREFIX), (*city).clone()).unwrap();
            drop(city);
            let cities: Vec<StandardListViewItem> = cities.iter().map(|&&x| x.into()).collect();
            ensure_run_in_event_loop(move |app| {
                app.set_cities(cities.as_slice().into());
                app.invoke_update_city(0);
            });
            load_data();
        });
    });
    init_city_country();
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
            prev_time = current_time;
            time_tx.send(current_time).unwrap();
            thread::sleep(SLEEP_TIME);
        }
    });
    thread::spawn(move || {
        load_data();
        let timings = TIMINGS.lock().unwrap();
        let current_prayer = timings[5].0.clone();
        let mut current_times: Vec<_> = timings.range(0..6).cloned().collect();
        for time in timings.range(0..6) {
            current_times[time.2] = time.clone();
        }
        drop(timings);
        let prayer_times: Vec<_> = current_times.iter().map(|x| PrayerTime{
            prayer: x.0.clone().into(),
            time: x.1.format("%I:%M").to_string().into(),
            ampm: x.1.format("%p").to_string().into(),
        }).collect();
        ensure_run_in_event_loop(move |app| {
            app.invoke_update_prayer_times(prayer_times.as_slice().into());
            app.invoke_update_current_prayer(current_prayer.into());
        });
        let mut tz = Local::now().format("%z").to_string();
        loop {
            let time = time_rx.recv().unwrap();
            let mut timings = TIMINGS.lock().unwrap();
            let new_tz = time.format("%z").to_string();
            if tz != new_tz {
                drop(timings);
                load_data();
                timings = TIMINGS.lock().unwrap();
                let current_prayer = timings[5].0.clone();
                for time in timings.range(0..6) {
                    current_times[time.2] = time.clone();
                }
                let prayer_times: Vec<_> = current_times.iter().map(|x| PrayerTime{
                    prayer: x.0.clone().into(),
                    time: x.1.format("%I:%M").to_string().into(),
                    ampm: x.1.format("%p").to_string().into(),
                }).collect();
                ensure_run_in_event_loop(move |app| {
                    app.invoke_update_prayer_times(prayer_times.as_slice().into());
                    app.invoke_update_current_prayer(current_prayer.into());
                });
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
            if timings[0] != current_times[timings[0].2] {
                let current_prayer = timings[5].0.clone();
                for time in timings.range(0..6) {
                    current_times[time.2] = time.clone();
                }
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
            if time >= timings[0].1 {
                let adhan = timings.pop_front().unwrap().0;
                log(format!("INFO: {} sent", adhan));
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
                ensure_run_in_event_loop(move |app| {
                    app.invoke_update_prayer_times(prayer_times.as_slice().into());
                    app.invoke_update_current_prayer(current_prayer.into());
                    app.set_adhan_playing(true);
                });
                if timings.len() <= 100 {
                    drop(timings);
                    load_data();
                    timings = TIMINGS.lock().unwrap();
                }
            }
        }
    });
    thread::spawn(move || {
        loop {
            log("INFO: Adhan loop advanced");
            let adhan = adhan_rx.recv().unwrap();
            log(format!("INFO: {} time", adhan));
            if adhan == "Sunrise" {
                srise_tx.send(true).unwrap();
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
    });
    thread::spawn(move || {
        loop {
            srise_rx.recv().unwrap();
            thread::sleep(Duration::from_mins(15));
            ensure_run_in_event_loop(move |app| {
                app.set_adhan_playing(false);
            });
        }
    });
    let app_clone = app.clone_strong();
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, Duration::from_millis(600), move || {
        let current_id = thread::current().id();
        if MAIN_THREAD.replace(current_id) != current_id {
            *WEAKAPP.lock().unwrap() = app_clone.as_weak();
            thread::spawn(|| log("INFO: Replaced WEAKAPP."));
        }
    });
    return (app, timer);
}

fn get_data() -> (String, String) {
    let mut year: u64 = format!("{}", Local::now().format("%Y")).parse().unwrap();
    let this_years_data;
    let country = COUNTRY.lock().unwrap();
    let city = CITY.lock().unwrap();
    if fs::exists(format!("{}{}-{}-{}.json", FILE_PREFIX, year, country, city)).unwrap() {
        this_years_data = fs::read_to_string(
            format!("{}{}-{}-{}.json", FILE_PREFIX, year, country, city),
        ).unwrap();
    } else {
        this_years_data = blocking::get(&format!(
            "https://api.aladhan.com/v1/calendar/{}?{}&method=2&school=1",
            year,
            CITIES[&country][&city],
        )).unwrap().text().unwrap();
        fs::write(
            format!("{}{}-{}-{}.json", FILE_PREFIX, year, country, city),
            this_years_data.clone(),
        ).unwrap();
    }
    year += 1;
    let next_years_data;
    if fs::exists(format!("{}{}-{}-{}.json", FILE_PREFIX, year, country, city)).unwrap() {
        next_years_data = fs::read_to_string(
            format!("{}{}-{}-{}.json", FILE_PREFIX, year, country, city),
        ).unwrap();
    } else {
        next_years_data = blocking::get(&format!(
            "https://api.aladhan.com/v1/calendar/{}?{}&method=2&school=1",
            year,
            CITIES[&country][&city],
        )).unwrap().text().unwrap();
        fs::write(
            format!("{}{}-{}-{}.json", FILE_PREFIX, year, country, city),
            next_years_data.clone(),
        ).unwrap();
    }
    return (this_years_data, next_years_data);
}

fn load_data() {
    let today = Local::now();
    let tz = today.format("%z").to_string();
    let (this_year, next_year) = get_data();
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
    let mut timings = TIMINGS.lock().unwrap();
    timings.clear();
    timings.extend(parsed_data);
}

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

fn load_location() {
    let mut city_data = "Dallas (Texas)".to_string();
    let mut country_data = "United States".to_string();
    let city_file = format!("{}city.txt", FILE_PREFIX);
    if fs::exists(&city_file).unwrap() {
        match fs::read_to_string(city_file) {
            Ok(x) => city_data = x,
            Err(e) => log(format!("Could not read city file because of {:#?}", e)),
        }
    }
    let country_file = format!("{}country.txt", FILE_PREFIX);
    if fs::exists(&country_file).unwrap() {
        match fs::read_to_string(country_file) {
            Ok(x) => country_data = x,
            Err(e) => log(format!("Could not read country file because of {:#?}", e)),
        }
    }
    let mut country = COUNTRY.lock().unwrap();
    *country = country_data;
    let mut city = CITY.lock().unwrap();
    *city = city_data;
}

fn init_city_country() {
    let mut countries: Vec<_> = CITIES.keys().collect();
    countries.sort();
    let country = COUNTRY.lock().unwrap();
    let country_pos = countries.binary_search(&&(*country).as_str()).unwrap();
    let mut cities: Vec<_> = CITIES[&*country].keys().collect();
    cities.sort();
    let city = CITY.lock().unwrap();
    let city_pos = cities.binary_search(&&(*city).as_str()).unwrap();
    let cities: Vec<StandardListViewItem> = cities.iter().map(|&&x| x.into()).collect();
    let countries: Vec<StandardListViewItem> = countries.iter().map(|&&x| x.into()).collect();
    ensure_run_in_event_loop(move |app| {
        app.set_city(city_pos as i32);
        app.set_country(country_pos as i32);
        app.set_cities(cities.as_slice().into());
        app.set_countries(countries.as_slice().into());
    });
}

fn ensure_run_in_event_loop<F>(func: F)
where F: FnOnce(Clock) + Send + Clone + 'static {
    loop {
        let result = Arc::new(Mutex::new(Some(())));
        let func = func.clone();
        let res_el = Arc::clone(&result);
        slint::invoke_from_event_loop(move || {
            let app = match WEAKAPP.lock().unwrap().upgrade() {
                Some(x) => x,
                None => {
                    *res_el.lock().unwrap() = None;
                    return;
                }
            };
            func(app);
        }).unwrap();
        match *result.lock().unwrap() {
            Some(_) => return,
            None => {
                thread::sleep(SLEEP_TIME);
                log(format!("INFO: Graphical update failed, retrying."));
            },
        }
    }
}
