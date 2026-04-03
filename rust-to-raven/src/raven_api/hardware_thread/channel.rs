
// extern crate alloc;

// use alloc::vec::Vec;

// use spin::mutex::SpinMutex;

// use alloc::sync::Arc;

// pub struct Channel<T> {
//     buffer: Vec<T>, //data buffer
// }

// impl<T> Channel<T> {

//     pub fn new() -> (Producer<T>, Consumer<T>) {
//         let channel = Channel {
//             buffer: Vec::new(),
//         };

//         let mut channel = Arc::new(SpinMutex::new(channel));
//         (Producer::new(&mut channel), Consumer::new(&mut channel))
//     }

//     #[unsafe(no_mangle)]
//     pub fn push(&mut self, value: T) {
//         self.buffer.push(value)
//     }
//     #[unsafe(no_mangle)]
//     pub fn pop(&mut self) -> Option<T> {
//         self.buffer.pop()
//     }
// }

// pub struct Producer<T> {
//     channel:Arc<SpinMutex<Channel<T>>>,
// }

// impl<T> Producer<T> {
    
//      fn new(channel: &mut Arc<SpinMutex<Channel<T>>>) -> Self {
//        let  mut c = channel.lock();

//        Self {
//             channel: channel.clone()
//        }
//     }

//     pub fn push(&mut self, value: T) {
//         self.channel.lock().push(value);
//     }
// }


// pub struct Consumer<T> {
//     channel: Arc<SpinMutex<Channel<T>>>,
// }

// impl<T> Consumer<T> {
    
//     fn new(channel: &mut Arc<SpinMutex<Channel<T>>>) -> Self {
//        let  mut c = channel.lock();

//        Self {
//             channel: channel.clone()
//        }
//     }

//     #[unsafe(no_mangle)] 
//     pub fn poll(&mut self) -> Option<T>{
//         self.channel.lock().pop()
//     }
// }


