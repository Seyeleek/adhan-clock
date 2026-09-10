use slint::language::StandardListViewItem;
use std::{
    thread,
    time::Duration,
    fs::{OpenOptions, self},
    sync::{mpsc, Mutex, Arc, LazyLock},
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

const ADHAN: &[u8] = include_bytes!("../adhan.ogg").as_slice();
const FAJR_ADHAN: &[u8] = include_bytes!("../fajr-adhan.ogg").as_slice();
const SLEEP_TIME: Duration = Duration::from_millis(500);

static LOG_LOCK: Mutex<()> = Mutex::new(());
static TIMINGS: Mutex<VecDeque<Timing>> = Mutex::new(VecDeque::new());
static COUNTRY: Mutex<String> = Mutex::new(String::new());
static COUNTRIES: LazyLock<Vec<&str>> = LazyLock::new(|| CITIES.keys().map(|x| *x).collect());
static CITY: Mutex<String> = Mutex::new(String::new());
static COUNTRY_CITIES: Mutex<Vec<&str>> = Mutex::new(Vec::new());
static SCHOOL: Mutex<String> = Mutex::new(String::new());
static WEAKAPP: LazyLock<Mutex<slint::Weak<Clock>>> = LazyLock::new(|| Mutex::new(Default::default()));
static RAN_BEFORE: Mutex<bool> = Mutex::new(false);
static DATA_LOCK: Mutex<u32> = Mutex::new(0);

type Timing = (String, DateTime<Local>, usize);

pub fn main_window() -> Clock {
    panic::set_hook(Box::new(move |panic_info| {
        log(
            format!("ERROR: {}\n{:#?}", panic_info, Backtrace::new()),
        );
    }));
    let app = Clock::new().unwrap();
    app.on_city_picked(move |new_city| {
        thread::spawn(move || {
            *CITY.lock().unwrap() = new_city.clone().into();
            fs::write(format!("{}city.txt", FILE_PREFIX), new_city).unwrap();
            load_data();
        });
    });
    app.on_school_picked(move |new_school| {
        thread::spawn(move || {
            let mut school = SCHOOL.lock().unwrap();
            *school = new_school.clone().into();
            fs::write(format!("{}school.txt", FILE_PREFIX), new_school).unwrap();
            drop(school);
            load_data();
        });
    });
    app.on_country_picked(move |new_country| {
        thread::spawn(move || {
            let mut country = COUNTRY.lock().unwrap();
            *country = new_country.clone().into();
            fs::write(format!("{}country.txt", FILE_PREFIX), new_country).unwrap();
            let cities: Vec<_> = CITIES[country.as_str()].keys().collect();
            drop(country);
            *COUNTRY_CITIES.lock().unwrap() = cities.clone();
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
    *WEAKAPP.lock().unwrap() = app.as_weak();
    let mut ran_before = RAN_BEFORE.lock().unwrap();
    if *ran_before {
        thread::spawn(|| log("INFO: Tried to run the app a second time."));
        thread::spawn(|| {
            init_city_country();
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
        });
        app.set_school((*SCHOOL.lock().unwrap()).clone().into());
        return app;
    } else {
        thread::spawn(|| log("INFO: Started the application."));
        *ran_before = true;
    }
    load_location_school();
    app.set_school((*SCHOOL.lock().unwrap()).clone().into());
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
    if SAFELY_PRUNE {
        thread::spawn(|| {
            let pattern = format!("{}*[!-][!l][!m].json", FILE_PREFIX);
            for file in glob::glob(pattern.as_str()).unwrap() {
                let file = file.unwrap();
                if fs::remove_file(&file).is_ok() {
                    log(format!("INFO: Removed {}.", file.display()));
                }
            }
        });
    }
    return app;
}

fn get_data() -> (String, String) {
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

fn get_year_data(year: i32, country: &String, city: &String, school: i32) -> String {
    let file = format!(
        "{}{}-{}-{}-{}-lm.json",
        FILE_PREFIX,
        year,
        country,
        city,
        school,
    );
    if fs::exists(&file).unwrap() {
        return fs::read_to_string(&file).unwrap();
    } else {
        let tmp = blocking::get(&format!(
            "https://api.aladhan.com/v1/calendar/{}?{}&school={}",
            year,
            CITIES[&country][&city],
            school,
        )).unwrap().text().unwrap();
        fs::write(&file, tmp.clone()).unwrap();
        return tmp;
    };
}

fn load_data() {
    let mut data_lock = DATA_LOCK.lock().unwrap();
    *data_lock += 1;
    let data_id = *data_lock;
    drop(data_lock);
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
    let data_lock = DATA_LOCK.lock().unwrap();
    if *data_lock == data_id {
        let mut timings = TIMINGS.lock().unwrap();
        timings.clear();
        timings.extend(parsed_data);
    }
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
                Err(e) => log(format!("Could not read {} file because of {:#?}", item.0, e)),
            }
        }
        *item.2.lock().unwrap() = value.into();
    }
}

fn init_city_country() {
    let country = COUNTRY.lock().unwrap();
    let country_pos = COUNTRIES.binary_search(&(country.as_str())).unwrap();
    let cities: Vec<_> = CITIES[&*country].keys().map(|x| *x).collect();
    *COUNTRY_CITIES.lock().unwrap() = cities.clone();
    let city = CITY.lock().unwrap();
    let city_pos = cities.binary_search(&(city.as_str())).unwrap();
    let cities: Vec<StandardListViewItem> = cities.iter().map(|&x| x.into()).collect();
    let countries: Vec<StandardListViewItem> = COUNTRIES.iter().map(|&x| x.into()).collect();
    ensure_run_in_event_loop(move |app| {
        app.set_city(city_pos as i32);
        app.set_country(country_pos as i32);
        app.set_cities(cities.as_slice().into());
        app.set_countries(countries.as_slice().into());
    });
}

fn ensure_run_in_event_loop<F>(func: F)
where F: FnOnce(Clock) + Send + Clone + 'static {
    let result = Arc::new(Mutex::new(Some(())));
    loop {
        let func = func.clone();
        let res_el = Arc::clone(&result);
        let app = WEAKAPP.lock().unwrap().clone();
        slint::invoke_from_event_loop(move || {
            let app = match app.upgrade() {
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
                thread::spawn(|| log("INFO: Graphical update failed, retrying."));
            },
        }
    }
}
