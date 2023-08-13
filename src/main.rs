use std::{
    collections::HashMap,
    env::{self},
    process::exit,
    thread,
    time::Duration,
};

use clap::Parser;
use rand::Rng;

use reqwest::header::HeaderMap;
use tokio::{
    fs::{File, OpenOptions},
    io::{stdout, AsyncBufReadExt, AsyncWriteExt, BufReader},
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

fn game_headers_map(cookie: &str) -> HeaderMap {
    let mut map = HeaderMap::new();
    for header in &GAME_HEADERS {
        map.insert(header.0, header.1.parse().unwrap());
    }
    map.insert("Cookie", cookie.parse().unwrap());

    map
}

fn answer_headers_map(cookie: &str) -> HeaderMap {
    let mut map = HeaderMap::new();
    for header in &ANSWER_HEADERS {
        map.insert(header.0, header.1.parse().unwrap());
    }
    map.insert("Cookie", cookie.parse().unwrap());
    map
}

async fn setup_answer_map(episode: &str) -> HashMap<i64, String> {
    let mut answers: HashMap<i64, String> = HashMap::new();

    let answers_buf = BufReader::new(File::open(format!("answers{}.txt", episode)).await.unwrap());
    let mut lines = answers_buf.lines();
    loop {
        let line = match lines.next_line().await {
            Ok(line) => match line {
                Some(line) => line,
                None => break,
            },
            Err(_) => break,
        };

        let first_space = line.find(' ').unwrap();
        let id = line[0..first_space].to_owned();
        let name = line[first_space + 1..line.len()].to_owned();
        answers.insert(id.parse().unwrap(), name);
    }

    answers
}

const MIN_RESULT_TIME: u64 = 2178;
const MAX_RESULT_TIME: u64 = 9653;

const MIN_ANSWER_TIME: u64 = 986;
const MAX_ANSWER_TIME: u64 = 4465;

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

    let mut rng = rand::thread_rng();

    let client = reqwest::Client::new();
    'game: loop {
        stdout()
            .write_all(format!("#НОВАЯ ИГРА#\n\n").as_bytes())
            .await
            .unwrap();

        let res: serde_json::Value = client
            .post("https://kp-guess-game-api.kinopoisk.ru/v1/games")
            .headers(game_headers_map(&cookie))
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
            let default_answer = res
                .get("stateData")
                .unwrap()
                .get("question")
                .unwrap()
                .get("answers")
                .unwrap()
                .as_array()
                .unwrap()[0]
                .as_str()
                .unwrap()
                .to_owned();

            let (val, old) = match answers.get(&id) {
                Some(val) => (val, true),
                None => (&default_answer, false),
            };

            let body = format!("{{\"answer\":{:?}}}", val);
            let res: serde_json::Value = client
                .post("https://kp-guess-game-api.kinopoisk.ru/v1/questions/answers")
                .headers(answer_headers_map(&cookie))
                .header("Content-Lenght", &body.len().to_string())
                .body(body)
                .send()
                .await
                .unwrap()
                .json()
                .await
                .unwrap();

            if !res.get("isCorrect").unwrap().as_bool().unwrap() {
                let correct_answer = res.get("correctAnswer").unwrap().as_str().unwrap();
                answers.insert(id, correct_answer.to_owned());
                answers_file
                    .write_all(format!("{id} {correct_answer}\n").as_bytes())
                    .await
                    .unwrap();

                stdout()
                    .write_all(format!("НОВЫЙ: {id} {correct_answer}\n").as_bytes())
                    .await
                    .unwrap();
            } else if !old {
                answers.insert(id, default_answer.to_owned());
                answers_file
                    .write_all(format!("{id} {default_answer}\n").as_bytes())
                    .await
                    .unwrap();
            } else {
                stdout()
                    .write_all(format!("СТАРЫЙ: {id} {val}\n").as_bytes())
                    .await
                    .unwrap();
            }

            let state_data = res.get("stateData").unwrap();

            let points = state_data.get("points").unwrap().as_i64().unwrap();

            stdout()
                .write_all(format!("ОЧКИ: {points}\n\n").as_bytes())
                .await
                .unwrap();

            id = match state_data.get("question") {
                Some(val) => val.get("id").unwrap().as_i64().unwrap(),
                None => match state_data.get("livesLeft").unwrap().as_i64().unwrap() {
                    0 => {
                        stdout()
                            .write_all(format!("ИГРА ОКОНЧЕНА. РЕЗУЛЬТАТ - {points}\n").as_bytes())
                            .await
                            .unwrap();
                        stdout()
                            .write_all(
                                format!("ВСЕГО ОТВЕТОВ СОБРАНО: {}\n\n", answers.len()).as_bytes(),
                            )
                            .await
                            .unwrap();

                        let waiting_time = rng.gen_range(MIN_RESULT_TIME..=MAX_RESULT_TIME);
                        thread::sleep(Duration::from_millis(waiting_time));

                        continue 'game;
                    }
                    _ => break 'game,
                },
            };

            let waiting_time = rng.gen_range(MIN_ANSWER_TIME..=MAX_ANSWER_TIME);
            thread::sleep(Duration::from_millis(waiting_time));
        }
    }

    println!("Похоже вы ответили на все вопросы.");
}
