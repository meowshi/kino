use std::{collections::HashMap, env, process::exit};

use clap::Parser;
use lazy_static::lazy_static;
use reqwest::header::{HeaderMap, HeaderValue};
use tokio::{
    fs::{File, OpenOptions},
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
};

const GAME_HEADERS: [(&str, &str); 13] = [
    ("Accept", "application/json, text/plain, */*"),
    ("Accept-Language", "en-US,en;q=0.9,ru;q=0.8"),
    ("Connection", "keep-alive"),
    ("Content-Length", "12"),
    ("Content-Type", "application/json"),
    ("Host", "kp-guess-game-api.kinopoisk.ru"),
    ("Origin", "https://www.kinopoisk.ru"),
    ("Referer", "https://www.kinopoisk.ru/"),
    ("Sec-Fetch-Dest", "empty"),
    ("Sec-Fetch-Mode", "cors"),
    ("Sec-Fetch-Site", "same-site"),
    ("sec-ch-ua-mobile", "?0"),
    ("sec-ch-ua-platform", "Windows"),
];

const ANSWER_HEADERS: [(&str, &str); 12] = [
    ("Accept", "application/json, text/plain, */*"),
    ("Accept-Language", "en-US,en;q=0.9,ru;q=0.8"),
    ("Connection", "keep-alive"),
    ("Content-Type", "application/json"),
    ("Host", "kp-guess-game-api.kinopoisk.ru"),
    ("Origin", "https://www.kinopoisk.ru"),
    ("Referer", "https://www.kinopoisk.ru/"),
    ("Sec-Fetch-Dest", "empty"),
    ("Sec-Fetch-Mode", "cors"),
    ("Sec-Fetch-Site", "same-site"),
    ("sec-ch-ua-mobile", "?0"),
    ("sec-ch-ua-platform", "Windows"),
];

lazy_static! {
    static ref GAME_HEADERS_MAP: HeaderMap<HeaderValue> = {
        let mut map = HeaderMap::new();
        for header in &GAME_HEADERS {
            map.insert(header.0, header.1.parse().unwrap());
        }
        map
    };
    static ref ANSWER_HEADERS_MAP: HeaderMap<HeaderValue> = {
        let mut map = HeaderMap::new();
        for header in &ANSWER_HEADERS {
            map.insert(header.0, header.1.parse().unwrap());
        }
        map
    };
}

async fn setup_answer_map(episode: &str) -> HashMap<i64, String> {
    let mut answers: HashMap<i64, String> = HashMap::new();

    let file_name = format!("answers{episode}.txt");
    let file = match File::open(&file_name).await {
        Ok(file) => file,
        Err(err) => match err.kind() {
            std::io::ErrorKind::NotFound => File::create(&file_name).await.unwrap(),
            _ => {
                eprintln!("Не получилось создать или открыть файл {file_name}. Попробуйте создать его вручную.");
                exit(0);
            }
        },
    };

    let answers_buf = BufReader::new(file);
    let mut lines = answers_buf.lines();
    loop {
        let line = match lines.next_line().await {
            Ok(line) => match line {
                Some(line) => line,
                None => break,
            },
            Err(_) => break,
        };

        let pair = line.split_once(' ').unwrap();
        let id = pair.0.to_owned();
        let name = pair.1.to_owned();

        answers.insert(id.parse().unwrap(), name);
    }

    answers
}

#[derive(Parser)]
struct Cli {
    #[arg(short, long)]
    episode: String,
}

impl Cli {
    fn check_episode(&self) {
        let episode: i32 = self.episode.parse().unwrap();
        if episode > 6 && episode < 1 {
            panic!("Введите верный номер эпизода.");
        }
    }
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    cli.check_episode();

    dotenv::dotenv().unwrap_or_else(|_| {
        println!(r#"Создайте файл ".env" и укажите "COOKIE=<ваши куки>""#);
        exit(0);
    });

    let cookie = env::var("COOKIE").unwrap_or_else(|_| {
        println!(r#"В файле ".env" укажите "COOKIE=<ваши куки>""#);
        exit(0);
    });

    let mut answers = setup_answer_map(&cli.episode).await;

    let mut answers_file = OpenOptions::new()
        .append(true)
        .open(format!("answers{}.txt", cli.episode))
        .await
        .unwrap();

    let client = reqwest::Client::new();

    'game: loop {
        let res: serde_json::Value = client
            .post("https://kp-guess-game-api.kinopoisk.ru/v1/games")
            .headers(GAME_HEADERS_MAP.clone())
            .header("Cookie", &cookie)
            .body(format!("{{\"gameId\":{}}}", cli.episode))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();

        let mut id: i64 = res
            .get("stateData")
            .unwrap()
            .get("question")
            .unwrap()
            .get("id")
            .unwrap()
            .as_i64()
            .unwrap();

        loop {
            let default_answer = String::new();
            let val = answers.get(&id).unwrap_or(&default_answer);

            let body = format!("{{\"answer\":{:?}}}", val);
            let res: serde_json::Value = client
                .post("https://kp-guess-game-api.kinopoisk.ru/v1/questions/answers")
                .headers(ANSWER_HEADERS_MAP.clone())
                .header("Content-Lenght", &body.len().to_string())
                .header("Cookie", &cookie)
                .body(body)
                .send()
                .await
                .unwrap()
                .json()
                .await
                .unwrap();

            if val.is_empty() {
                let correct_answer = res.get("correctAnswer").unwrap().as_str().unwrap();
                answers.insert(id, correct_answer.to_owned());
                answers_file
                    .write_all(format!("{id} {correct_answer}\n").as_bytes())
                    .await
                    .unwrap();
            }

            let state_data = res.get("stateData").unwrap();

            id = match state_data.get("question") {
                Some(val) => val.get("id").unwrap().as_i64().unwrap(),
                None => match state_data.get("livesLeft").unwrap().as_i64().unwrap() {
                    0 => continue 'game,
                    _ => break 'game,
                },
            };
        }
    }

    println!("Похоже вы ответили на все вопросы");
}
