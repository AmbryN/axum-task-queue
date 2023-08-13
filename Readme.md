# Threaded task queue with Rust and Axum

Attempt at creating a prototype of task processing queue with the Web Server Axum which uses Threads in order to process tasks in parallel.

The `create_task` handler function creates a task from the payload and passes it down a mpsc channel for a feeder thread to pick up.
The feeder thread blocks on the receiving end of the channel and picks up each task when it becomes available.
It then passes it to a threadpool with a specified amount of worker thread, which passes the task down another mpsc channel for the worker thread to pick up and compute.

## Built With

* [Rust](https://www.rust-lang.org/)
* [Axum](https://github.com/tokio-rs/axum)

## Authors

* **AmbryN** - *Initial work* - [AmbryN](https://github.com/AmbryN)
