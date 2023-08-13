use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use axum_macros::debug_handler;
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    net::SocketAddr,
    sync::{atomic::AtomicU64, Arc, RwLock}
};

struct AppState {
    created_tasks: AtomicU64,
    queue: RwLock<VecDeque<Task>>,
    running_tasks: RwLock<Vec<Task>>,
    finished_tasks: RwLock<Vec<Task>>,
}

impl AppState {
    fn new() -> AppState {
        AppState {
            created_tasks: AtomicU64::new(0),
            queue: RwLock::new(VecDeque::new()),
            running_tasks: RwLock::new(Vec::new()),
            finished_tasks: RwLock::new(Vec::new()),
        }
    }
}
type SharedAppState = Arc<AppState>;

const MAX_WORKER_THREAD: u8 = 10;

#[tokio::main]
async fn main() {
    // initialize tracing
    tracing_subscriber::fmt::init();

    // Initialiazing the shared state of the application
    let shared_state: SharedAppState = Arc::new(AppState::new());

    // Spawn MAX_WORKER_THREAD threads
    for i in 0..MAX_WORKER_THREAD {
        let worker_state = Arc::clone(&shared_state);
        std::thread::spawn(move || {
            println!("Start of the computation thread {}", i);
            loop {
                let mut queue = worker_state.queue.write().unwrap();
                if let Some(mut task) = queue.pop_front() {
                    drop(queue);

                    task.state = TaskState::InProgress;

                    let mut running_tasks = worker_state.running_tasks.write().unwrap();
                    running_tasks.push(task.clone());
                    drop(running_tasks);

                    println!("Thread {i} : Beginning of task {}", task.duration);
                    compute(&mut task);
                    println!("Thread {i} : Result {:?}", task.result);

                    task.state = TaskState::Finished;

                    let mut running_tasks = worker_state.running_tasks.write().unwrap();
                    let index = running_tasks
                        .iter()
                        .position(|x| *x == task)
                        .expect("needle not found");
                    running_tasks.remove(index);
                    drop(running_tasks);

                    worker_state.finished_tasks.write().unwrap().push(task);
                }
            }
        });
    }

    // Add routes to application
    let app = Router::new()
        .route("/tasks", get(tasks))
        .route("/tasks", post(create_task))
        .with_state(Arc::clone(&shared_state));

    // Listen on port 3000
    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    tracing::debug!("listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

#[debug_handler]
async fn tasks(state: State<SharedAppState>) -> Json<Vec<Task>> {
    let queue: Vec<Task> = state.queue.read().unwrap().clone().into();
    let running_tasks = state.running_tasks.read().unwrap().clone();
    let finished_tasks = state.finished_tasks.read().unwrap().to_vec();
    Json([queue, running_tasks, finished_tasks].concat())
}

#[debug_handler]
async fn create_task(state: State<SharedAppState>, Json(payload): Json<CreateTask>) -> Json<Task> {
    let created_tasks = state
        .created_tasks
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let task = Task::new(created_tasks, payload.duration);

    let task_to_return = task.clone();
    state.queue.write().unwrap().push_back(task);

    Json(task_to_return)
}

fn compute(task: &mut Task) {
    let duration: u64 = task.duration as u64;
    // std::thread::sleep(Duration::from_secs(duration));
    let result = fibonacci(duration);
    task.result = Some(result);
}

fn fibonacci(nb: u64) -> u64 {
    let (mut x, mut y) = (0, 1);
    for _ in 0..10_000_000 {
        (x, y) = (0, 1);
        for _ in 0..nb {
            let temp = x;
            x = y;
            y = x + temp;
        }
    }
    return x;
}

#[derive(Debug, Clone, Serialize)]
struct Task {
    id: u64,
    duration: u64,
    result: Option<u64>,
    state: TaskState,
}

impl Task {
    fn new(id: u64, duration: u64) -> Task {
        Task {
            id: id,
            duration: duration,
            result: None,
            state: TaskState::NotStarted,
        }
    }
}

impl PartialEq for Task {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

#[derive(Debug, Clone, Deserialize)]
struct CreateTask {
    duration: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
enum TaskState {
    NotStarted,
    InProgress,
    Cancelled,
    Finished,
    Error,
}
