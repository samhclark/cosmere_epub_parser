use std::{
    error::Error,
    fs::{self, File},
    io::{BufReader, Write},
    path::{Path, PathBuf},
};

use epub::doc::EpubDoc;
use html2text::{from_read_with_decorator, render::text_renderer::TextDecorator};
use serde::Serialize;

#[derive(Debug)]
struct IndexableBook {
    title: String,
    first_chapter_index: usize,
    last_chapter_index: usize,
    skippable_chapters: Vec<usize>,
}

#[derive(Debug, Serialize)]
struct OutputSchema {
    book_title: String,
    chapter_title: String,
    searchable_text: String,
    display_text: String,
}

fn main() -> Result<(), Box<dyn Error>> {
    let all_books: Vec<IndexableBook> = vec![
        IndexableBook {
            title: String::from("The Alloy of Law"),
            first_chapter_index: 7,
            last_chapter_index: 32,
            skippable_chapters: vec![10, 16, 22, 26],
        },
        IndexableBook {
            title: String::from("Shadows of Self"),
            first_chapter_index: 7,
            last_chapter_index: 37,
            skippable_chapters: vec![8, 13, 31],
        },
        IndexableBook {
            title: String::from("The Bands of Mourning"),
            first_chapter_index: 7,
            last_chapter_index: 42,
            skippable_chapters: vec![8, 13, 26],
        },
        IndexableBook {
            title: String::from("Secret History"),
            first_chapter_index: 5,
            last_chapter_index: 35,
            skippable_chapters: vec![7, 12, 16, 21, 25],
        },
        IndexableBook {
            title: String::from("Warbreaker"),
            first_chapter_index: 5,
            last_chapter_index: 65,
            skippable_chapters: vec![],
        },
        IndexableBook {
            title: String::from("The Emperor's Soul"),
            first_chapter_index: 3,
            last_chapter_index: 18,
            skippable_chapters: vec![],
        },
        IndexableBook {
            title: String::from("The Hope of Elantris"),
            first_chapter_index: 28,
            last_chapter_index: 28,
            skippable_chapters: vec![],
        },
    ];

    let path = Path::new("output.json");
    let display = path.display();

    // Open a file in write-only mode, returns `io::Result<File>`
    let file = match File::create(path) {
        Err(why) => panic!("couldn't create {display}: {why}"),
        Ok(file) => file,
    };

    let epub_files: Vec<PathBuf> = fs::read_dir(".")?
        .flatten()
        .filter(|it| it.file_type().unwrap().is_file())
        .filter(|it| it.file_name().to_str().unwrap().ends_with("epub"))
        .map(|it| it.path().canonicalize().unwrap())
        .collect();

    for path in epub_files {
        let doc = EpubDoc::new(path);
        let epub_title = doc
            .as_ref()
            .unwrap()
            .mdata("title")
            .expect("All ePubs must have a title");
        println!("Found epub titled: {epub_title}");
        if let Some(book) = all_books.iter().find(|it| {
            epub_title
                .to_lowercase()
                .contains(it.title.to_lowercase().as_str())
                || (epub_title.contains("Arcanum Unbounded")
                    && is_in_arcanum_unbounded(&it.title))
        }) {
            parse_and_write_book(book, doc.unwrap(), &file);
        }
    }

    Ok(())
}

fn parse_and_write_book(
    book: &IndexableBook,
    mut doc: EpubDoc<BufReader<File>>,
    mut outfile: &File,
) {
    println!("Parsing {}", book.title);
    for chapter_index in book.first_chapter_index..=book.last_chapter_index {
        if book.skippable_chapters.contains(&chapter_index) {
            continue;
        }
        doc.set_current_page(chapter_index)
            .expect("Indexes used in `skippable_chapters` must be valid");
        let chapter_title = doc.spine[chapter_index].clone();
        let this_page_raw = doc.get_current().unwrap();
        let this_page = String::from_utf8(this_page_raw).unwrap();
        let this_page_replaced = this_page
            .replace("<i>", "<em>")
            .replace("</i>", "</em>")
            .replace("<img", "<img alt=\"795f88d2-e400-42f0-bb88-d84cf308de1b\"")
            .replace("<p class=\"Part-Title-pt\"><a href=\"contents.xhtml#c_pt3\"><span class=\"ePub-SC\">THE</span><br/>HOPE<br/><span class=\"ePub-SC\">OF</span><br/>ELANTRIS</a></p>", "")
            .replace("<p class=\"Design-Note-dn\"><span class=\"R1\">This story takes place after and contains major spoilers for</span> <span class=\"ePub-I\">Elantris.</span></p>", "");
        let page_content = from_read_with_decorator(
            this_page_replaced.as_bytes(),
            usize::MAX,
            MyDecorator::new(),
        );
        // println!("{}", page_content);
        let lines_i_care_about: Vec<String> = page_content
            .lines()
            .filter(|it| !is_ignorable_line(it))
            .filter(|it| !chapter_title.ends_with(it))
            .map(|it| it.replace("**", ""))
            .map(|it| it.replace(". . .", "…"))
            .map(|it| it.replace(" …", "…"))
            .collect();
        for values in lines_i_care_about.windows(3) {
            let prev = values
                .first()
                .expect(".windows() returns exactly 3 elements");
            let curr = values
                .get(1)
                .expect(".windows() returns exactly 3 elements");
            let next = values
                .get(2)
                .expect(".windows() returns exactly 3 elements");

            if is_scene_border(curr) {
                continue;
            }

            // handle scene divisions
            let prev_line = if is_scene_border(prev) || is_ignorable_line(prev) {
                String::new()
            } else {
                format!("{prev}</p><p>")
            };

            let next_line = if is_scene_border(next) || is_ignorable_line(next) {
                String::new()
            } else {
                format!("</p><p>{next}")
            };

            let paragraph_with_context = format!("{prev_line}{curr}{next_line}");

            let out = OutputSchema {
                book_title: book.title.clone(),
                chapter_title: pretty_chapter(&book.title, &chapter_title),
                searchable_text: curr.clone().replace("<em>", "").replace("</em>", ""),
                display_text: paragraph_with_context,
            };
            let mut json = serde_json::to_string(&out).unwrap();
            json.push('\n');
            outfile.write_all(json.as_bytes()).unwrap();
        }
    }
}

fn is_ignorable_line(line: &str) -> bool {
    let trimmed = line.trim();

    trimmed.is_empty()
        || trimmed.starts_with('#')
        || trimmed.starts_with('│')
        || trimmed.starts_with("─┴")
        || trimmed.starts_with("─┬")
}

fn is_scene_border(line: &str) -> bool {
    let borders = vec!["* * *", "~", "795f88d2-e400-42f0-bb88-d84cf308de1b"];
    borders.contains(&line)
}

#[allow(clippy::case_sensitive_file_extension_comparisons)]
fn pretty_chapter(book_title: &str, raw_chapter: &str) -> String {
    if book_title.eq_ignore_ascii_case("The Hope of Elantris") {
        String::new()
    } else if raw_chapter.to_ascii_lowercase() == "prologue" {
        String::from("Prologue")
    } else if raw_chapter.to_ascii_lowercase() == "epilogue" {
        String::from("Epilogue")
    } else if raw_chapter.to_ascii_lowercase().starts_with("chapter") {
        let num: String = raw_chapter
            .chars()
            .into_iter()
            .filter(char::is_ascii_digit)
            .collect();
        format!("Chapter {}", num.trim_start_matches('0'))
    } else if raw_chapter.starts_with('x') && raw_chapter.ends_with(".html") {
        handle_secret_history_chapter(raw_chapter)
    } else {
        String::from(map_by_hand(raw_chapter))
    }
}

fn handle_secret_history_chapter(raw_chapter: &str) -> String {
    let part_number = raw_chapter.chars().nth(1).unwrap();
    let chapter_number = raw_chapter.chars().nth(3).unwrap();
    format!("Part {part_number}, Chapter {chapter_number}")
}

fn map_by_hand(raw_chapter: &str) -> &str {
    match raw_chapter {
        "Prologue.html" => "Prologue",
        "Day_02.html" => "Day Two",
        "Day_03.html" => "Day Three",
        "Day_05.html" => "Day Five",
        "Day_12.html" => "Day Twelve",
        "Day_17.html" => "Day Seventeen",
        "Day_30.html" => "Day Thirty",
        "Day_42.html" => "Day Forty-Two",
        "Day_58.html" => "Day Fifty-Eight",
        "Day_59.html" => "Day Fifty-Nine",
        "Day_70.html" => "Day Seventy",
        "Day_76.html" => "Day Seventy-Six",
        "Day_85.html" => "Day Eighty-Five",
        "Day_97.html" => "Day Ninety-Seven",
        "Day_98.html" => "Day Ninety-Eight",
        "Epilogue.html" => "Epilogue: Day One Hundred and One",
        _ => raw_chapter,
    }
}

fn is_in_arcanum_unbounded(title: &str) -> bool {
    match title {
        "The Hope of Elantris"
        | "The Eleventh Metal"
        | "Allomancer Jak and the Pits of Eltania"
        | "White Sand"
        | "Shadows for Silence in the Forests of Hell"
        | "Sixth of the Dusk"
        | "Edgedancer" => true,
        _ => false,
    }
}

#[derive(Clone, Debug)]
struct MyDecorator {}

impl MyDecorator {
    pub const fn new() -> Self {
        Self {}
    }
}

impl TextDecorator for MyDecorator {
    type Annotation = ();

    fn decorate_code_start(&mut self) -> (String, Self::Annotation) {
        (String::new(), ())
    }

    fn decorate_code_end(&mut self) -> String {
        String::new()
    }

    fn decorate_em_start(&mut self) -> (String, Self::Annotation) {
        (String::from("<em>"), ())
    }

    fn decorate_em_end(&mut self) -> String {
        String::from("</em>")
    }

    fn decorate_image(&mut self, _title: &str) -> (String, Self::Annotation) {
        (String::new(), ())
    }

    fn decorate_link_start(&mut self, _url: &str) -> (String, Self::Annotation) {
        (String::new(), ())
    }

    fn decorate_link_end(&mut self) -> String {
        String::new()
    }

    fn decorate_preformat_first(&mut self) -> Self::Annotation {}

    fn decorate_preformat_cont(&mut self) -> Self::Annotation {}

    fn decorate_strikeout_start(&mut self) -> (String, Self::Annotation) {
        (String::from("<s>"), ())
    }

    fn decorate_strikeout_end(&mut self) -> String {
        String::from("</s>")
    }

    fn decorate_strong_start(&mut self) -> (String, Self::Annotation) {
        (String::new(), ())
    }

    fn decorate_strong_end(&mut self) -> String {
        String::new()
    }

    fn header_prefix(&mut self, _level: usize) -> String {
        String::new()
    }

    fn quote_prefix(&mut self) -> String {
        String::new()
    }

    fn ordered_item_prefix(&mut self, _i: i64) -> String {
        String::new()
    }

    fn unordered_item_prefix(&mut self) -> String {
        String::new()
    }

    fn finalise(self) -> Vec<html2text::render::text_renderer::TaggedLine<Self::Annotation>> {
        Vec::new()
    }

    fn make_subblock_decorator(&self) -> Self {
        Self::new()
    }
}
