#[macro_use]
extern crate systemd;
#[macro_use]
extern crate log;
use systemd::journal::{self, Journal, JournalFiles, SeekRet};

// #[test]
fn test() {
    use systemd::journal;
    journal::send(&["CODE_FILE=HI", "CODE_LINE=1213", "CODE_FUNCTION=LIES"]);
    journal::print(1, &format!("Rust can talk to the journal: {}", 4));

    journal::JournalLog::init().ok().unwrap();
    log!(log::LogLevel::Info, "HI");
    sd_journal_log!(4, "HI {:?}", 2);
}

#[test]
fn seek_test() {
    let cursor = "s=7fdbf48af0cd4ebea663b48532f0f2a9;i=1b48;b=9a9e16e612fa4e308846351376210c0f;\
                  m=28eb92beb;t=531da2dfe278d;x=2e356bd88254f838"
                     .to_string();

    let mut client = match Journal::open(JournalFiles::All, false, true) {
        Ok(c) => c,
        Err(e) => {
            println!("Error opening");
            panic!("Couldn't create client. Error = {:?}", e);
        }
    };

    match client.seek(cursor.clone()) {
        Ok(r) => {
            if r == SeekRet::ClosestSeek {
                println!("Invalid cursor. Seeking to closest\n. cursor = {}", cursor);
            } else if r == SeekRet::SeekSuccess {
                println!("Seek success");
            }
        }
        Err(e) => println!("Error seeking cursor. e = {}\n. cursor = {}", e, cursor),
    };
}

// #[test]
fn iterator_test() {
    let mut client = match Journal::open(JournalFiles::All, false, true) {
        Ok(c) => c,
        Err(e) => {
            println!("Error opening");
            panic!("Couldn't create client. Error = {:?}", e);
        }
    };
    client.set_iterator_timeout(10);
    let mut count = 0;
    for (j, c) in &client {
        count += 1;
        println!("{:?}. {:?}", count, j);
        println!("time stamp = {:?}", client.get_realtime_us());
        println!("");
    }
}
