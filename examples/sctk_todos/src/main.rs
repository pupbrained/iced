use env_logger::Env;
use iced::alignment::{self, Alignment};
use iced::event::{self, Event};
use iced::id::Id;
use iced::theme::{self, Theme};
use iced::wayland::actions::data_device::ActionInner;
use iced::wayland::actions::window::SctkWindowSettings;
use iced::wayland::data_device::action as data_device_action;
use iced::wayland::InitialSurface;
use iced::widget::{
    self, button, checkbox, column, container, row, scrollable, text,
    text_input, Text,
};
use iced::{id, keyboard};
use iced::{subscription, Limits};
use iced::{window, Application, Element};
use iced::{Color, Command, Font, Length, Settings, Subscription};

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::sync::Arc;

static INPUT_ID: Lazy<Id> = Lazy::new(Id::unique);

pub fn main() -> iced::Result {
    let env = Env::default()
        .filter_or("MY_LOG_LEVEL", "info")
        .write_style_or("MY_LOG_STYLE", "always");

    let mut settings = SctkWindowSettings::default();
    settings.size_limits = Limits::NONE.min_height(300.0).min_width(600.0);
    env_logger::init_from_env(env);
    Todos::run(Settings {
        initial_surface: InitialSurface::XdgWindow(settings),
        ..Settings::default()
    })
}

#[derive(Debug)]
enum Todos {
    Loading,
    Loaded(State),
}

#[derive(Debug, Default, Clone)]
enum DndState {
    #[default]
    None,
    // hold onto text input state and the id of the window being dragged
    Dragging(text_input::State, window::Id),
}

#[derive(Debug, Default)]
struct State {
    window_id_ctr: u128,
    input_value: String,
    filter: Filter,
    tasks: Vec<Task>,
    dirty: bool,
    saving: bool,
    dnd_state: DndState,
}

#[derive(Clone)]
enum Message {
    Loaded(Result<SavedState, LoadError>),
    Saved(Result<(), SaveError>),
    InputChanged(String),
    CreateTask,
    FilterChanged(Filter),
    TaskMessage(usize, TaskMessage),
    TabPressed { shift: bool },
    CloseRequested(window::Id),
    TextInputDragged(text_input::State),
    TextInputDndCommand(Arc<Box<dyn Send + Sync + Fn() -> ActionInner>>),
}

impl Debug for Message {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Message::Loaded(_) => write!(f, "Message::Loaded(_)"),
            Message::Saved(_) => write!(f, "Message::Saved(_)"),
            Message::InputChanged(_) => write!(f, "Message::InputChanged(_)"),
            Message::CreateTask => write!(f, "Message::CreateTask"),
            Message::FilterChanged(_) => write!(f, "Message::FilterChanged(_)"),
            Message::TaskMessage(_, _) => {
                write!(f, "Message::TaskMessage(_, _)")
            }
            Message::TabPressed { shift: _ } => {
                write!(f, "Message::TabPressed {{ shift: _ }}")
            }
            Message::CloseRequested(_) => {
                write!(f, "Message::CloseRequested(_)")
            }
            Message::TextInputDragged(_) => {
                write!(f, "Message::TextInputDragged(_)")
            }
            Message::TextInputDndCommand(_) => {
                write!(f, "Message::TextInputDndCommand(_)")
            }
        }
    }
}

impl Application for Todos {
    type Message = Message;
    type Theme = Theme;
    type Executor = iced::executor::Default;
    type Flags = ();

    fn new(_flags: ()) -> (Todos, Command<Message>) {
        (
            Todos::Loading,
            Command::batch(vec![Command::perform(
                SavedState::load(),
                Message::Loaded,
            )]),
        )
    }

    fn title(&self) -> String {
        let dirty = match self {
            Todos::Loading => false,
            Todos::Loaded(state) => state.dirty,
        };

        format!("Todos{} - Iced", if dirty { "*" } else { "" })
    }

    fn update(&mut self, message: Message) -> Command<Message> {
        match self {
            Todos::Loading => {
                match message {
                    Message::Loaded(Ok(state)) => {
                        *self = Todos::Loaded(State {
                            input_value: state.input_value,
                            filter: state.filter,
                            tasks: state.tasks,
                            window_id_ctr: 1,
                            ..State::default()
                        });
                    }
                    Message::Loaded(Err(_)) => {
                        *self = Todos::Loaded(State::default());
                    }
                    _ => {}
                }

                text_input::focus(INPUT_ID.clone())
            }
            Todos::Loaded(state) => {
                let mut saved = false;

                let command = match message {
                    Message::InputChanged(value) => {
                        state.input_value = value;

                        Command::none()
                    }
                    Message::CreateTask => {
                        if !state.input_value.is_empty() {
                            state
                                .tasks
                                .push(Task::new(state.input_value.clone()));
                            state.input_value.clear();
                        }
                        Command::none()
                    }
                    Message::FilterChanged(filter) => {
                        state.filter = filter;

                        Command::none()
                    }
                    Message::TaskMessage(i, TaskMessage::Delete) => {
                        state.tasks.remove(i);

                        Command::none()
                    }
                    Message::TaskMessage(i, task_message) => {
                        if let Some(task) = state.tasks.get_mut(i) {
                            let should_focus =
                                matches!(task_message, TaskMessage::Edit);

                            task.update(task_message);

                            if should_focus {
                                let id = Task::text_input_id(i);
                                Command::batch(vec![
                                    text_input::focus(id.clone()),
                                    text_input::select_all(id),
                                ])
                            } else {
                                Command::none()
                            }
                        } else {
                            Command::none()
                        }
                    }
                    Message::Saved(_) => {
                        state.saving = false;
                        saved = true;

                        Command::none()
                    }
                    Message::TabPressed { shift } => {
                        if shift {
                            widget::focus_previous()
                        } else {
                            widget::focus_next()
                        }
                    }
                    Message::CloseRequested(_) => {
                        std::process::exit(0);
                    }
                    Message::TextInputDragged(text_input_state) => {
                        let icon_id = window::Id(state.window_id_ctr);

                        state.dnd_state = DndState::Dragging(
                            text_input_state.clone(),
                            icon_id,
                        );

                        Command::none()
                    }
                    Message::TextInputDndCommand(action) => {
                        data_device_action(action())
                    }
                    _ => Command::none(),
                };

                if !saved {
                    state.dirty = true;
                }

                let save = if state.dirty && !state.saving {
                    state.dirty = false;
                    state.saving = true;

                    Command::perform(
                        SavedState {
                            input_value: state.input_value.clone(),
                            filter: state.filter,
                            tasks: state.tasks.clone(),
                        }
                        .save(),
                        Message::Saved,
                    )
                } else {
                    Command::none()
                };

                Command::batch(vec![command, save])
            }
        }
    }

    fn view(&self, id: window::Id) -> Element<Message> {
        match self {
            Todos::Loading => loading_message(),
            Todos::Loaded(State {
                input_value,
                filter,
                tasks,
                dnd_state,
                window_id_ctr,
                ..
            }) => {
                match dnd_state {
                    DndState::Dragging(state, drag_id) if *drag_id == id => {
                        return text_input(
                            "What needs to be done?",
                            &state.dragged_text().unwrap_or_default(),
                        )
                        .on_input(Message::InputChanged)
                        .dnd_icon(true)
                        .size(30)
                        .into();
                    }
                    _ => {}
                };
                if window::Id(0) != id {
                    panic!("Wrong window id: {:?}", id)
                }

                let title = text("todos")
                    .width(Length::Fill)
                    .size(100)
                    .style(Color::from([0.5, 0.5, 0.5]))
                    .horizontal_alignment(alignment::Horizontal::Center);

                let input = text_input("What needs to be done?", input_value)
                    .id(INPUT_ID.clone())
                    .padding(15)
                    .size(30)
                    .on_submit(Message::CreateTask)
                    .on_input(Message::InputChanged)
                    .on_paste(Message::InputChanged)
                    .surface_ids((window::Id(0), window::Id(*window_id_ctr)))
                    .on_start_dnd(Message::TextInputDragged)
                    .on_dnd_command_produced(|a| {
                        Message::TextInputDndCommand(Arc::new(a))
                    });

                let controls = view_controls(tasks, *filter);
                let filtered_tasks =
                    tasks.iter().filter(|task| filter.matches(task));

                let tasks: Element<_> = if filtered_tasks.count() > 0 {
                    column(
                        tasks
                            .iter()
                            .enumerate()
                            .filter(|(_, task)| filter.matches(task))
                            .map(|(i, task)| {
                                task.view(i).map(move |message| {
                                    Message::TaskMessage(i, message)
                                })
                            })
                            .collect(),
                    )
                    .spacing(10)
                    .into()
                } else {
                    empty_message(match filter {
                        Filter::All => "You have not created a task yet...",
                        Filter::Active => "All your tasks are done! :D",
                        Filter::Completed => {
                            "You have not completed a task yet..."
                        }
                    })
                };

                let content = column![title, input, controls, tasks]
                    .spacing(20)
                    .max_width(800);

                scrollable(
                    container(content)
                        .width(Length::Fill)
                        .padding(40)
                        .center_x(),
                )
                .into()
            }
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        subscription::events_with(|event, status| match (event, status) {
            (
                Event::Keyboard(keyboard::Event::KeyPressed {
                    key_code: keyboard::KeyCode::Tab,
                    modifiers,
                    ..
                }),
                event::Status::Ignored,
            ) => Some(Message::TabPressed {
                shift: modifiers.shift(),
            }),
            (
                Event::PlatformSpecific(event::PlatformSpecific::Wayland(
                    event::wayland::Event::Window(e, s, id),
                )),
                _,
            ) => {
                dbg!(&e);
                None
            }
            _ => None,
        })
    }

    fn close_requested(&self, id: window::Id) -> Self::Message {
        Message::CloseRequested(id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Task {
    description: String,
    completed: bool,

    #[serde(skip)]
    state: TaskState,
}

#[derive(Debug, Clone)]
pub enum TaskState {
    Idle,
    Editing,
}

impl Default for TaskState {
    fn default() -> Self {
        Self::Idle
    }
}

#[derive(Debug, Clone)]
pub enum TaskMessage {
    Completed(bool),
    Edit,
    DescriptionEdited(String),
    FinishEdition,
    Delete,
}

impl Task {
    fn text_input_id(i: usize) -> id::Id {
        id::Id::new(format!("task-{}", i))
    }

    fn new(description: String) -> Self {
        Task {
            description,
            completed: false,
            state: TaskState::Idle,
        }
    }

    fn update(&mut self, message: TaskMessage) {
        match message {
            TaskMessage::Completed(completed) => {
                self.completed = completed;
            }
            TaskMessage::Edit => {
                self.state = TaskState::Editing;
            }
            TaskMessage::DescriptionEdited(new_description) => {
                self.description = new_description;
            }
            TaskMessage::FinishEdition => {
                if !self.description.is_empty() {
                    self.state = TaskState::Idle;
                }
            }
            TaskMessage::Delete => {}
        }
    }

    fn view(&self, i: usize) -> Element<TaskMessage> {
        match &self.state {
            TaskState::Idle => {
                let checkbox = checkbox(
                    &self.description,
                    self.completed,
                    TaskMessage::Completed,
                )
                .width(Length::Fill);

                row![
                    checkbox,
                    button(edit_icon())
                        .on_press(TaskMessage::Edit)
                        .padding(10)
                        .style(theme::Button::Text),
                ]
                .spacing(20)
                .align_items(Alignment::Center)
                .into()
            }
            TaskState::Editing => {
                let text_input =
                    text_input("Describe your task...", &self.description)
                        .id(Self::text_input_id(i))
                        .on_submit(TaskMessage::FinishEdition)
                        .on_input(TaskMessage::DescriptionEdited)
                        .on_paste(TaskMessage::DescriptionEdited)
                        .padding(10);

                row![
                    text_input,
                    button(row![delete_icon(), "Delete"].spacing(10))
                        .on_press(TaskMessage::Delete)
                        .padding(10)
                        .style(theme::Button::Destructive)
                ]
                .spacing(20)
                .align_items(Alignment::Center)
                .into()
            }
        }
    }
}

fn view_controls(tasks: &[Task], current_filter: Filter) -> Element<Message> {
    let tasks_left = tasks.iter().filter(|task| !task.completed).count();

    let filter_button = |label, filter, current_filter| {
        let label = text(label).size(16);

        let button = button(label).style(if filter == current_filter {
            theme::Button::Primary
        } else {
            theme::Button::Text
        });

        button.on_press(Message::FilterChanged(filter)).padding(8)
    };

    row![
        text(format!(
            "{} {} left",
            tasks_left,
            if tasks_left == 1 { "task" } else { "tasks" }
        ))
        .width(Length::Fill)
        .size(16),
        row![
            filter_button("All", Filter::All, current_filter),
            filter_button("Active", Filter::Active, current_filter),
            filter_button("Completed", Filter::Completed, current_filter,),
        ]
        .width(Length::Shrink)
        .spacing(10)
    ]
    .spacing(20)
    .align_items(Alignment::Center)
    .into()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Filter {
    All,
    Active,
    Completed,
}

impl Default for Filter {
    fn default() -> Self {
        Filter::All
    }
}

impl Filter {
    fn matches(&self, task: &Task) -> bool {
        match self {
            Filter::All => true,
            Filter::Active => !task.completed,
            Filter::Completed => task.completed,
        }
    }
}

fn loading_message<'a>() -> Element<'a, Message> {
    container(
        text("Loading...")
            .horizontal_alignment(alignment::Horizontal::Center)
            .size(50),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .center_y()
    .into()
}

fn empty_message(message: &str) -> Element<'_, Message> {
    container(
        text(message)
            .width(Length::Fill)
            .size(25)
            .horizontal_alignment(alignment::Horizontal::Center)
            .style(Color::from([0.7, 0.7, 0.7])),
    )
    .width(Length::Fill)
    .height(Length::Fixed(200.0))
    .center_y()
    .into()
}

// Fonts
const ICONS: Font = Font::with_name("Iced-Todos-Icons");

fn icon(unicode: char) -> Text<'static> {
    text(unicode.to_string())
        .font(ICONS)
        .width(Length::Fixed(20.0))
        .horizontal_alignment(alignment::Horizontal::Center)
        .size(20)
}

fn edit_icon() -> Text<'static> {
    icon('\u{F303}')
}

fn delete_icon() -> Text<'static> {
    icon('\u{F1F8}')
}

// Persistence
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SavedState {
    input_value: String,
    filter: Filter,
    tasks: Vec<Task>,
}

#[derive(Debug, Clone)]
enum LoadError {
    File,
    Format,
}

#[derive(Debug, Clone)]
enum SaveError {
    File,
    Write,
    Format,
}

#[cfg(not(target_arch = "wasm32"))]
impl SavedState {
    fn path() -> std::path::PathBuf {
        let mut path = if let Some(project_dirs) =
            directories_next::ProjectDirs::from("rs", "Iced", "Todos")
        {
            project_dirs.data_dir().into()
        } else {
            std::env::current_dir().unwrap_or_default()
        };

        path.push("todos.json");

        path
    }

    async fn load() -> Result<SavedState, LoadError> {
        use async_std::prelude::*;

        let mut contents = String::new();

        let mut file = async_std::fs::File::open(Self::path())
            .await
            .map_err(|_| LoadError::File)?;

        file.read_to_string(&mut contents)
            .await
            .map_err(|_| LoadError::File)?;

        serde_json::from_str(&contents).map_err(|_| LoadError::Format)
    }

    async fn save(self) -> Result<(), SaveError> {
        use async_std::prelude::*;

        let json = serde_json::to_string_pretty(&self)
            .map_err(|_| SaveError::Format)?;

        let path = Self::path();

        if let Some(dir) = path.parent() {
            async_std::fs::create_dir_all(dir)
                .await
                .map_err(|_| SaveError::File)?;
        }

        {
            let mut file = async_std::fs::File::create(path)
                .await
                .map_err(|_| SaveError::File)?;

            file.write_all(json.as_bytes())
                .await
                .map_err(|_| SaveError::Write)?;
        }

        // This is a simple way to save at most once every couple seconds
        async_std::task::sleep(std::time::Duration::from_secs(2)).await;

        Ok(())
    }
}

#[cfg(target_arch = "wasm32")]
impl SavedState {
    fn storage() -> Option<web_sys::Storage> {
        let window = web_sys::window()?;

        window.local_storage().ok()?
    }

    async fn load() -> Result<SavedState, LoadError> {
        let storage = Self::storage().ok_or(LoadError::File)?;

        let contents = storage
            .get_item("state")
            .map_err(|_| LoadError::File)?
            .ok_or(LoadError::File)?;

        serde_json::from_str(&contents).map_err(|_| LoadError::Format)
    }

    async fn save(self) -> Result<(), SaveError> {
        let storage = Self::storage().ok_or(SaveError::File)?;

        let json = serde_json::to_string_pretty(&self)
            .map_err(|_| SaveError::Format)?;

        storage
            .set_item("state", &json)
            .map_err(|_| SaveError::Write)?;

        let _ = wasm_timer::Delay::new(std::time::Duration::from_secs(2)).await;

        Ok(())
    }
}
