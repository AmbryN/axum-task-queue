use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use axum_macros::debug_handler;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    net::SocketAddr,
    sync::{atomic::AtomicU64, Arc, RwLock},
};
use std::{
    hash::{Hash, Hasher},
    sync::{
        mpsc::{self, Sender},
        Mutex,
    },
};
use task_queue::ThreadPool;

struct AppState {
    tx_task: Mutex<Sender<Task>>,
    created_tasks: AtomicU64,
    running_tasks: RwLock<HashSet<Task>>,
    finished_tasks: RwLock<HashSet<Task>>,
}

impl AppState {
    fn new(tx: Sender<Task>) -> AppState {
        AppState {
            tx_task: Mutex::new(tx),
            created_tasks: AtomicU64::new(0),
            running_tasks: RwLock::new(HashSet::new()),
            finished_tasks: RwLock::new(HashSet::new()),
        }
    }
}
type SharedAppState = Arc<AppState>;

const MAX_CONCURRENT_THREADS: usize = 1;

#[tokio::main]
async fn main() {
    // initialize tracing
    tracing_subscriber::fmt::init();

    // Create the channel for passing tasks
    let (tx, rx) = mpsc::channel::<Task>();

    // Create the thread pool for computing the tasks
    let thread_pool = ThreadPool::new(MAX_CONCURRENT_THREADS);

    // Initialiazing the shared state of the application
    let shared_state: SharedAppState = Arc::new(AppState::new(tx));

    // Spawn feeder thread
    let worker_state = Arc::clone(&shared_state);
    std::thread::spawn(move || loop {
        // Read task from the channel (blocks until a task is available)
        let mut task = rx.recv().unwrap();

        // Copy the state to pass it into the worker thread
        let worker_state = Arc::clone(&worker_state);

        // Add a task to the thread pool for computation
        thread_pool.execute(move || {
            // Add the task to the running tasks to keep track of their status
            task.state = TaskState::InProgress;

            worker_state
                .running_tasks
                .write()
                .unwrap()
                .insert(task.clone());

            // Start computing the task
            println!(
                "Beginning of task {} with iterations {}",
                task.id, task.duration
            );
            compute(&mut task);
            println!("Task {} has Result {:?}", task.id, task.result);
            task.state = TaskState::Finished;

            // Remove the task from the running tasks
            worker_state.running_tasks.write().unwrap().remove(&task);

            // Add the task to the finished tasks
            worker_state.finished_tasks.write().unwrap().insert(task);
        });
    });

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
    // Fetch all the task : running and finished
    let running_tasks: Vec<Task> = state
        .running_tasks
        .read()
        .unwrap()
        .clone()
        .into_iter()
        .collect();
    let finished_tasks = state
        .finished_tasks
        .read()
        .unwrap()
        .clone()
        .into_iter()
        .collect();
    Json([running_tasks, finished_tasks].concat())
}

#[debug_handler]
async fn create_task(state: State<SharedAppState>, Json(payload): Json<CreateTask>) -> Json<Task> {
    // Create a task from the payload
    let created_tasks = state
        .created_tasks
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let task = Task::new(created_tasks, payload.duration);

    let task_to_return = task.clone();

    // Pass task into the channel
    state.tx_task.lock().unwrap().send(task).unwrap();

    Json(task_to_return)
}

fn compute(task: &mut Task) {
    let duration: u64 = task.duration as u64;
    let result = nth_prime(duration);
    task.result = Some(result);
}

fn nth_prime(mut n: u64) -> u64 {
    let mut i = 2;
    while n > 0 {
        if is_prime(i) {
            n -= 1;
        }
        i += 1;
    }
    i -= 1;
    return i;
}

fn is_prime(n: u64) -> bool {
    if n <= 1 {
        return false;
    }
    if n == 2 || n == 3 {
        return true;
    }
    // below 5 there is only two prime numbers 2 and 3
    if n % 2 == 0 || n % 3 == 0 {
        return false;
    }
    // Using concept of prime number can be represented
    // in form of (6*k + 1) or(6*k - 1)
    for i in (5..(1 + ((n as f64).powf(0.5)) as u64)).step_by(6) {
        if n % i == 0 || n % (i + 2) == 0 {
            return false;
        }
    }
    return true;
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

impl Eq for Task {}

impl Hash for Task {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

#[derive(Debug, Clone, Deserialize)]
struct CreateTask {
    duration: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, Hash)]
enum TaskState {
    NotStarted,
    InProgress,
    Cancelled,
    Finished,
    Error,
}
