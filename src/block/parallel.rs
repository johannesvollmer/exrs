use std::marker::PhantomData;
use std::sync::mpsc::{Sender, Receiver};

pub struct ParallelPipe<Source, Result, Fn> {
    converter: Fn,
    thread_pool: rayon::ThreadPool,
    result_sender: Sender<Result>,
    result_receiver: Receiver<Result>,
    source: PhantomData<Source>,
    current_item_count: usize,
    max_item_count: usize,
}

impl<Src, Res, F> ParallelPipe<Src, Res, F> where F: Send + Fn(Src) -> Res {

    /// None if a thread pool cannot be created.
    pub fn new(max_thread_count: usize, converter: F) -> Option<Self> {
        let thread_pool = rayon::ThreadPoolBuilder::new()
            .panic_handler()
            .build()
            .ok()?;

        let (result_sender, result_receiver) = std::sync::mpsc::channel();

        Some(Self {
            converter: F,
            source: PhantomData::default(),
            max_item_count: max_thread_count,
            current_item_count: 0,
            thread_pool,
            result_sender,
            result_receiver,
        })
    }

    pub fn advance(&mut self, item: Src) -> Res {

    }

    pub fn push_blocking(&mut self, item: Src) {

    }

    pub fn pull_blocking(&mut self) -> Option<Res> {

    }
}



