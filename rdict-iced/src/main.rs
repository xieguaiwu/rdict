#![windows_subsystem = "windows"]

use anyhow::Result;
use directories_next::ProjectDirs;
use iced::font;
use iced::widget::{button, column, container, row, rule, scrollable, space, text, text_input};
use iced::window::Settings;
use iced::{Alignment, Element, Font, Length, Size, Task};
use rdict_core::parse::TranslationData;
use rdict_core::rdict::{FetchedResult, Rdict};

#[derive(Default)]
struct State {
    text_input_content: String,
    translation_result: TranslationState,
    client: Option<Rdict>,
}

#[derive(Default)]
enum TranslationState {
    #[default]
    Empty,
    Loading,
    Error(String),
    Translation(FetchedResult),
}

#[derive(Debug, Clone)]
enum Message {
    ContentChanged(String),
    Submit,
    TranslationResult(FetchedResult),
    TranslationError(String),
}

fn main() -> Result<()> {
    // https://www.reddit.com/r/learnrust/comments/jaqfcx/windows_print_to_hidden_console_window/
    #[cfg(target_os = "windows")]
    {
        use winapi::um::wincon::{ATTACH_PARENT_PROCESS, AttachConsole};
        unsafe {
            let _ = AttachConsole(ATTACH_PARENT_PROCESS);
        }
    }

    iced::application(State::default, update, view)
        .title("Rdict")
        .window(Settings {
            size: Size::new(400.0, 600.0),
            min_size: Some(Size::new(200.0, 200.0)),
            ..Settings::default()
        })
        .run()?;

    Ok(())
}

fn update(state: &mut State, message: Message) -> Task<Message> {
    match message {
        Message::ContentChanged(content) => {
            state.text_input_content = content;
            Task::none()
        }
        Message::Submit => {
            if state.text_input_content.trim().is_empty() {
                return Task::none();
            }
            state.translation_result = TranslationState::Loading;

            // Clone only what is required for the async move block
            let text_input_content = state.text_input_content.clone();
            let client = state.client.clone();

            Task::perform(
                async move { fetch_translation(client, text_input_content).await },
                |res| match res {
                    Ok(msg) => Message::TranslationResult(msg),
                    Err(e) => Message::TranslationError(e.to_string()),
                },
            )
        }
        Message::TranslationResult(msg) => {
            state.translation_result = TranslationState::Translation(msg);
            Task::none()
        }
        Message::TranslationError(err) => {
            state.translation_result = TranslationState::Error(err);
            Task::none()
        }
    }
}

fn view(state: &State) -> Element<'_, Message> {
    let main: Element<'_, Message> = match &state.translation_result {
        TranslationState::Empty => space().width(Length::Fill).height(Length::Fill).into(),

        TranslationState::Error(error) => container(column![
            text("Lookup Error")
                .style(text::danger)
                .font(Font {
                    weight: font::Weight::Bold,
                    ..Font::default()
                })
                .size(20),
            text(error).size(16).style(text::secondary),
        ])
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .into(),

        TranslationState::Loading => container(text("Loading...").size(16).style(text::secondary))
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .into(),

        TranslationState::Translation(fetched_result) => match &fetched_result.data {
            TranslationData::ToChinese(tc) => {
                // Pronunciation Layout
                let pronunciation_col = match (&tc.pronunciation.uk, &tc.pronunciation.us) {
                    (Some(uk), Some(us)) => Some(
                        row![
                            row![
                                text("英").font(Font {
                                    weight: font::Weight::Bold,
                                    ..Font::default()
                                }),
                                " ",
                                text(format!("[{uk}]")).style(text::secondary)
                            ],
                            row![
                                text("美").font(Font {
                                    weight: font::Weight::Bold,
                                    ..Font::default()
                                }),
                                " ",
                                text(format!("[{us}]")).style(text::secondary)
                            ]
                        ]
                        .spacing(15),
                    ),
                    (Some(uk), None) => Some(row![
                        text("英").font(Font {
                            weight: font::Weight::Bold,
                            ..Font::default()
                        }),
                        " ",
                        text(format!("[{uk}]")).style(text::secondary)
                    ]),
                    (None, Some(us)) => Some(row![
                        text("美").font(Font {
                            weight: font::Weight::Bold,
                            ..Font::default()
                        }),
                        " ",
                        text(format!("[{us}]")).style(text::secondary)
                    ]),
                    (None, None) => None,
                };

                // Meanings Layout
                let mut meanings_col = column![
                    text("Meanings").style(text::secondary).font(Font {
                        weight: font::Weight::Bold,
                        ..Font::default()
                    }),
                    rule::horizontal(1)
                ]
                .spacing(10);

                for meaning in &tc.meanings {
                    let mut definitions_col = column![].spacing(2);
                    if let Some(p) = &meaning.part_of_speech {
                        definitions_col = definitions_col.push(
                            container(text(p))
                                .padding([4, 8])
                                .style(container::bordered_box),
                        );
                    }
                    for definition in &meaning.definitions {
                        definitions_col = definitions_col.push(list_item(text(definition)));
                    }
                    meanings_col = meanings_col.push(definitions_col);
                }

                // Examples Layout
                let mut examples_col = column![
                    text("Examples").style(text::secondary).font(Font {
                        weight: font::Weight::Bold,
                        ..Font::default()
                    }),
                    rule::horizontal(2)
                ]
                .spacing(10);

                for example in &tc.examples {
                    examples_col = examples_col.push(
                        column![
                            text(&example.en).font(Font {
                                weight: font::Weight::Medium,
                                ..Font::default()
                            }),
                            text(&example.zh).size(14).style(text::secondary)
                        ]
                        .spacing(5),
                    );
                }

                scrollable(
                    column![
                        text(&tc.input_text).size(40).font(Font {
                            weight: font::Weight::ExtraBold,
                            ..Font::default()
                        }),
                        pronunciation_col,
                        meanings_col,
                        examples_col
                    ]
                    .spacing(20)
                    .padding(10),
                )
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
            }

            TranslationData::ToEnglish(te) => {
                // Meanings Layout
                let mut meanings_col = column![
                    text("Meanings").style(text::secondary).font(Font {
                        weight: font::Weight::Bold,
                        ..Font::default()
                    }),
                    rule::horizontal(1)
                ]
                .spacing(10);

                for meaning in &te.meanings {
                    meanings_col = meanings_col.push(list_item(text(meaning)));
                }

                // Examples Layout
                let mut examples_col = column![
                    text("Examples").style(text::secondary).font(Font {
                        weight: font::Weight::Bold,
                        ..Font::default()
                    }),
                    rule::horizontal(1)
                ]
                .spacing(10);

                for example in &te.examples {
                    examples_col = examples_col.push(
                        column![
                            text(&example.zh).font(Font {
                                weight: font::Weight::Medium,
                                ..Font::default()
                            }),
                            text(&example.en).size(14).style(text::secondary)
                        ]
                        .spacing(5),
                    );
                }

                scrollable(
                    column![
                        text(&te.input_text).size(40).font(Font {
                            weight: font::Weight::ExtraBold,
                            ..Font::default()
                        }),
                        meanings_col,
                        examples_col
                    ]
                    .spacing(20)
                    .padding(10),
                )
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
            }
        },
    };

    let mut layout = column![
        row![
            text_input("Type something here...", &state.text_input_content)
                .on_input(Message::ContentChanged)
                .on_submit(Message::Submit),
            button("Translate").on_press(Message::Submit),
        ]
        .spacing(5),
        main
    ]
    .padding(10)
    .spacing(10);

    if cfg!(debug_assertions) {
        layout = layout.push(text("rdict_iced dev").align_x(iced::alignment::Horizontal::Center));
    }

    layout.into()
}

async fn fetch_translation(
    client: Option<Rdict>,
    text_input_content: String,
) -> Result<FetchedResult, rdict_core::Error> {
    let client = if let Some(c) = client {
        c
    } else {
        let cache_db_path = ProjectDirs::from("dev", "ny4", "rdict")
            .map(|proj_dirs| proj_dirs.cache_dir().join("cache.db"));
        Rdict::new("https://m.youdao.com", cache_db_path).await?
    };

    client.get_results(&text_input_content).await
}

fn list_item<'a, Message: 'static>(
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    row![
        // FIXME: use proper way to render the dot
        text("•").size(20),
        content.into()
    ]
    .spacing(10)
    .align_y(Alignment::Center)
    .into()
}
