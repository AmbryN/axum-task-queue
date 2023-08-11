use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use axum_macros::debug_handler;
use serde::{Deserialize, Serialize};
use std::{
    net::SocketAddr,
    sync::{Arc, RwLock},
    time::Duration,
};

struct AppState {
    queue: RwLock<Queue>,
    finished_tasks: RwLock<Vec<Task>>,
}

impl AppState {
    fn new() -> AppState {
        AppState {
            queue: RwLock::new(Queue::new()),
            finished_tasks: RwLock::new(Vec::new()),
        }
    }
}
type SharedAppState = Arc<AppState>;

const MAX_WORKER_THREAD: u8 = 5;

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
            {
                println!("Start of the computation thread {}", i);
                loop {
                    let mut queue = worker_state.queue.write().unwrap();
                    if let Some(mut task) = queue.dequeue() {
                        drop(queue);

                        println!("Thread {i} : Beginning of task {}", task.duration);
                        compute(&mut task);
                        println!("Thread {i} : Result {:?}", task.result);
                        
                        worker_state.finished_tasks.write().unwrap().push(task);
                    }
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
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    tracing::debug!("listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

#[debug_handler]
async fn tasks(state: State<SharedAppState>) -> Json<Vec<Task>> {
    let queue = state.queue.read().unwrap().tasks.to_vec();
    let finished_tasks = state.finished_tasks.read().unwrap().to_vec();
    Json([queue, finished_tasks].concat())
}

#[debug_handler]
async fn create_task(state: State<SharedAppState>, Json(payload): Json<CreateTask>) -> Json<Task> {
    let task = Task {
        duration: payload.duration,
        result: None,
        state: TaskState::NotStarted
    };

    let task_to_return = task.clone();
    state.queue.write().unwrap().enqueue(task);

    Json(task_to_return)
}

fn compute(task: &mut Task) {
    let duration: u64 = task.duration as u64;
    task.state = TaskState::InProgress;
    std::thread::sleep(Duration::from_secs(duration));
    task.result = Some(duration);
    task.state = TaskState::Finished;
}

#[derive(Debug, Clone, Serialize)]
struct Task {
    duration: u64,
    result: Option<u64>,
    state: TaskState
}

#[derive(Debug, Clone, Deserialize)]
struct CreateTask {
    duration: u64,
}

#[derive(Debug, Clone, Serialize)]
enum TaskState {
    NotStarted,
    InProgress,
    Cancelled,
    Finished,
    Error
}

#[derive(Debug, Default)]
struct Queue {
    tasks: Vec<Task>,
}

impl Queue {
    fn new() -> Queue {
        Queue {
            tasks: Vec::new(),
        }
    }

    fn enqueue(&mut self, task: Task) {
        self.tasks.insert(0, task);
    }

    fn dequeue(&mut self) -> Option<Task> {
        self.tasks.pop()
    }

    fn size(&self) -> usize {
        self.tasks.len()
    }
}
