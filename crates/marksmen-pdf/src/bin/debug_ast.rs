use pulldown_cmark::{Parser, Options, Event, Tag};
use std::fs;

fn main() {
    let text = fs::read_to_string("report/ARPA-H_SonALAsense_Milestone 12 Report.md").unwrap();
    
    // We only care about the region around Table 10.
    // Let's just parse the full thing and find the events.
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_HEADING_ATTRIBUTES);
    
    let parser = Parser::new_ext(&text, options);
    
    let mut count = 0;
    let mut last_event = None;
    for (idx, event) in parser.enumerate() {
        count += 1;
        last_event = Some(event);
    }
    println!("Total events: {}", count);
    if let Some(e) = last_event {
        println!("Last event: {:?}", e);
    }
}
