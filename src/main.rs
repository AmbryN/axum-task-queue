use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use axum_macros::debug_handler;
use serde::{Deserialize, Serialize};
use std::{
    collections::{VecDeque, HashSet},
    net::SocketAddr,
    sync::{atomic::AtomicU64, Arc, RwLock},
    time::Duration,
};

struct AppState {
    threads: AtomicU64,
    created_tasks: AtomicU64,
    queue: RwLock<VecDeque<Task>>,
    running_tasks: RwLock<HashSet<Task>>,
    finished_tasks: RwLock<HashSet<Task>>,
}

impl AppState {
    fn new() -> AppState {
        AppState {
            threads: AtomicU64::new(0),
            created_tasks: AtomicU64::new(0),
            queue: RwLock::new(VecDeque::new()),
            running_tasks: RwLock::new(HashSet::new()),
            finished_tasks: RwLock::new(HashSet::new()),
        }
    }
}
type SharedAppState = Arc<AppState>;

const MAX_WORKER_THREADS: u64 = 8;

#[tokio::main]
async fn main() {
    // initialize tracing
    tracing_subscriber::fmt::init();

    // Initialiazing the shared state of the application
    let shared_state: SharedAppState = Arc::new(AppState::new());

    // Spawn feeder thread
    let worker_state = Arc::clone(&shared_state);
    std::thread::spawn(move || loop {
        let current_number_threads = worker_state
        .threads
        .load(std::sync::atomic::Ordering::Relaxed);
    
    if current_number_threads < MAX_WORKER_THREADS {
            let mut queue = worker_state.queue.write().unwrap();
            if let Some(mut task) = queue.pop_front() {
                // Drop the queue to release the lock
                drop(queue);

                // Increase the number of running threads
                let current_thread = worker_state
                    .threads
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

                // Copy the state to pass it into the worker thread
                let worker_state = Arc::clone(&worker_state);
                std::thread::spawn(move || {
                    println!("Start of the worker thread {}", current_number_threads);

                    // Add the task to the running tasks to keep track of their status
                    task.state = TaskState::InProgress;
                    let mut running_tasks = worker_state.running_tasks.write().unwrap();
                    running_tasks.insert(task.clone());
                    drop(running_tasks);

                    // Start computing the task
                    println!(
                        "Thread {current_thread} : Beginning of task {} with iterations {}",
                        task.id, task.duration
                    );
                    compute(&mut task);
                    println!("Thread {current_thread} : Task {} has Result {:?}", task.id, task.result);
                    task.state = TaskState::Finished;

                    // Remove the task from the running tasks
                    let mut running_tasks = worker_state.running_tasks.write().unwrap();
                    // let index = running_tasks
                    //     .iter()
                    //     .position(|x| *x == task)
                    //     .expect("needle not found");
                    running_tasks.remove(&task);
                    drop(running_tasks);
                    
                    // Add the task to the finished tasks
                    worker_state.finished_tasks.write().unwrap().insert(task);

                    // Reduce the number of working threads
                    worker_state
                        .threads
                        .fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
                });
            } else {
                // Drop the queue to release the lock before waiting
                drop(queue);
                // Sleep in order to reduce impact of infinite loop
                std::thread::sleep(Duration::from_millis(100))
            }
        } else {
            // Sleep in order to reduce impact of infinite loop
            std::thread::sleep(Duration::from_millis(100))
        }
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
    // Fetch all the task : queued, running and finished
    let queue: Vec<Task> = state.queue.read().unwrap().clone().into();
    let running_tasks = state.running_tasks.read().unwrap().clone().into_iter().collect();
    let finished_tasks = state.finished_tasks.read().unwrap().clone().into_iter().collect();
    Json([queue, running_tasks, finished_tasks].concat())
}

#[debug_handler]
async fn create_task(state: State<SharedAppState>, Json(payload): Json<CreateTask>) -> Json<Task> {
    // Create a task from the payload
    let created_tasks = state
        .created_tasks
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let task = Task::new(created_tasks, payload.duration);

    let task_to_return = task.clone();
    // Queue the task
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
    // Long running computation
    for _ in 0..20_000_000 {
        (x, y) = (0, 1);
        for _ in 0..nb {
            let temp = x;
            x = y;
            y = x + temp;
        }
    }
    return x;
}

#[derive(Debug, Clone, Serialize, Hash, Eq)]
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

#[derive(Debug, Clone, Serialize, PartialEq, Eq, Hash)]
enum TaskState {
    NotStarted,
    InProgress,
    Cancelled,
    Finished,
    Error,
}
