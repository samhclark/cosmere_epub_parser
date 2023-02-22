use std::{
    error::Error,
    fs::{self, File},
    io::{BufReader, Write},
    path::{Path, PathBuf},
};

use epub::doc::EpubDoc;
use html2text::from_read;
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
            last_chapter_index: 66,
            skippable_chapters: vec![],
        },
    ];

    let path = Path::new("output.json");
    let display = path.display();

    // Open a file in write-only mode, returns `io::Result<File>`
    let file = match File::create(path) {
        Err(why) => panic!("couldn't create {}: {}", display, why),
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
        let full_title = doc
            .as_ref()
            .unwrap()
            .mdata("title")
            .expect("All ePubs must have a title");
        if let Some(book) = all_books.iter().find(|it| {
            full_title
                .to_lowercase()
                .contains(it.title.to_lowercase().as_str())
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
    for chapter_index in book.first_chapter_index..=book.last_chapter_index {
        if book.skippable_chapters.contains(&chapter_index) {
            continue;
        }
        doc.set_current_page(chapter_index)
            .expect("Indexes used in `skippable_chapters` must be valid");
        let chapter_title = doc.spine[chapter_index].clone();
        let this_page = doc.get_current().unwrap();
        let page_content = from_read(&this_page[..], usize::MAX);
        let lines_i_care_about: Vec<String> = page_content
            .lines()
            .filter(|it| !it.trim().is_empty())
            .filter(|it| !it.trim().starts_with('['))
            .filter(|it| !it.trim().starts_with('<'))
            .filter(|it| !it.trim().starts_with('#'))
            .filter(|it| !chapter_title.ends_with(it))
            .map(|it| it.replace("*", ""))
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

            if is_scene_border(&curr) {
                continue;
            }

            // handle scene divisions
            let prev_line = if is_scene_border(&prev) {
                String::new()
            } else {
                format!("{}</p><p>", prev)
            };

            let next_line = if is_scene_border(&next) {
                String::new()
            } else {
                format!("</p><p>{}", next)
            };

            
            let paragraph_with_context = format!("{}{}{}", prev_line, curr, next_line);

            let out = OutputSchema {
                book_title: book.title.clone(),
                chapter_title: pretty_chapter(&chapter_title),
                searchable_text: curr.clone(),
                display_text: paragraph_with_context,
            };
            let mut json = serde_json::to_string(&out).unwrap();
            json.push('\n');
            outfile.write_all(json.as_bytes()).unwrap();
        }
    }
}

fn is_scene_border(line: &str) -> bool {
    let borders = vec!["* * *", "~"];
    borders.contains(&line)
}

fn pretty_chapter(raw_chapter: &str) -> String {
    if raw_chapter.to_ascii_lowercase() == "prologue" {
        String::from("Prologue")
    } else if raw_chapter.to_ascii_lowercase() == "epilogue" {
        String::from("Epilogue")
    } else if raw_chapter.to_ascii_lowercase().starts_with("chapter") {
        let num: String = raw_chapter
            .chars()
            .into_iter()
            .filter(char::is_ascii_digit)
            .collect();
        format!("Chapter {num}")
    } else if raw_chapter.starts_with('x') && raw_chapter.ends_with(".html") {
        handle_secret_history_chapter(raw_chapter)
    } else {
        String::from(raw_chapter)
    }
}

fn handle_secret_history_chapter(raw_chapter: &str) -> String {
    let part_number = raw_chapter.chars().nth(1).unwrap();
    let chapter_number = raw_chapter.chars().nth(3).unwrap();
    format!("Part {}, Chapter {}", part_number, chapter_number)
}
