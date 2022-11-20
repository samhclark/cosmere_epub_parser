use std::{error::Error, fs::{self, File}, path::{PathBuf, Path}, io::{BufReader, Write}};

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
    ];

    let path = Path::new("output.json");
    let display = path.display();

    // Open a file in write-only mode, returns `io::Result<File>`
    let file = match File::create(&path) {
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
        let full_title = doc.as_ref()
            .unwrap()
            .mdata("title")
            .expect("All ePubs must have a title");
        if let Some(book) = all_books.iter().find(|it| {
            full_title
                .to_lowercase()
                .contains(it.title.to_lowercase().as_str())
        }) {
            parse_and_write_book(&book, doc.unwrap(), &file);
        }
    }

    Ok(())
}

fn parse_and_write_book(book: &IndexableBook, mut doc: EpubDoc<BufReader<File>>, mut outfile: &File) {

    for chapter_index in book.first_chapter_index..=book.last_chapter_index {
        if book.skippable_chapters.contains(&chapter_index) {
            continue;
        }
        doc.set_current_page(chapter_index)
            .expect("Indexes used in `skippable_chapters` must be valid");
        let chapter_title = doc.spine[chapter_index].clone();
        let this_page = doc.get_current().unwrap();
        let page_content = from_read(&this_page[..], usize::MAX);
        for (i, line) in page_content.lines()
            .filter(|it| !it.trim().is_empty())
            .filter(|it| !it.trim().starts_with('['))
            .filter(|it| !it.trim().starts_with('<'))
            .enumerate() {
            let prev: Option<&str> = if i > 0 {
                page_content.lines().nth(i - 1)
            } else {
                None
            };
            let next: Option<&str> = page_content.lines().nth(i + 1);
            let prev_line = prev.map_or_else(String::new, |s| format!("{}</p><p>", s));
            let next_line = next.map_or_else(String::new, |s| format!("</p><p>{}", s));
            let paragraph_with_context = format!("{}{}{}", prev_line, line, next_line);

            let out = OutputSchema {
                book_title: book.title.clone(),
                chapter_title: chapter_title.clone(),
                searchable_text: paragraph_with_context, 
            };
            let mut json = serde_json::to_string(&out).unwrap();
            json.push('\n');
            outfile.write_all(json.as_bytes()).unwrap();
        }
    }
}
