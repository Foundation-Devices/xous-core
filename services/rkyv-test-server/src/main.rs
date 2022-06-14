#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]

mod api;
use num_traits::{FromPrimitive, ToPrimitive};
use xous_ipc::{Buffer, String};

fn value_or(val: Option<i32>, default: api::MathResult) -> api::MathResult {
    val.map(|v| api::MathResult::Value(v)).unwrap_or(default)
}

fn handle_math_withcopy(mem: &mut xous::MemoryMessage) {
    let mut buffer = unsafe { Buffer::from_memory_message_mut(mem) };
    let response = {
        use api::MathOperation::*;
        match buffer.to_original().unwrap() {
            Add(a, b) => value_or(
                a.checked_add(b),
                api::MathResult::Error(api::Error::Overflow),
            ),
            Subtract(a, b) => value_or(
                a.checked_sub(b),
                api::MathResult::Error(api::Error::Underflow),
            ),
            Multiply(a, b) => value_or(
                a.checked_mul(b),
                api::MathResult::Error(api::Error::Overflow),
            ),
            Divide(a, b) => value_or(
                a.checked_div(b),
                api::MathResult::Error(api::Error::Underflow),
            ),
        }
    };
    buffer.replace(response).unwrap();
}

// This doesn't deserialize the struct, and therefore operates entirely
// on the archived data. This saves a copy step.
fn handle_math_zerocopy(mem: &mut xous::MemoryMessage) {
    let mut buffer = unsafe { Buffer::from_memory_message_mut(mem) };
    let response = {
        use api::ArchivedMathOperation::*;
        match *buffer.as_flat::<api::MathOperation, _>().unwrap() {
            Add(a, b) => value_or(
                a.checked_add(b),
                api::MathResult::Error(api::Error::Overflow),
            ),
            Subtract(a, b) => value_or(
                a.checked_sub(b),
                api::MathResult::Error(api::Error::Underflow),
            ),
            Multiply(a, b) => value_or(
                a.checked_mul(b),
                api::MathResult::Error(api::Error::Overflow),
            ),
            Divide(a, b) => value_or(
                a.checked_div(b),
                api::MathResult::Error(api::Error::Underflow),
            ),
        }
    };
    buffer.replace(response).unwrap();
}

fn handle_log_string(mem: &xous::MemoryMessage) {
    let buffer = unsafe { Buffer::from_memory_message(mem) };
    let log_string = buffer.as_flat::<api::LogString, _>().unwrap();
    log::info!(
        "Prefix: {}  Message: {}",
        log_string.prefix.as_str(),
        log_string.message.as_str()
    );
}

/// Take the given string and double each character in an output string.
fn double_string(mem: &mut xous::MemoryMessage) {
    use core::fmt::Write;
    let mut buffer = unsafe { Buffer::from_memory_message_mut(mem) };
    let mut response = api::StringDoubler {
        value: String::new(),
    };
    for ch in buffer
        .as_flat::<api::StringDoubler, _>()
        .unwrap()
        .value
        .as_str()
        .chars()
    {
        write!(response.value, "{}{}", ch, ch).ok();
    }
    buffer.replace(response).unwrap();
}

fn local_lender() {
    use core::fmt::Write;

    let ticktimer = ticktimer_server::Ticktimer::new().unwrap();
    let mut message_string = xous::String::<64>::new();

    let mut idx: usize = 0;
    loop {
        message_string.clear();
        write!(message_string, "LOCAL loop # {:^4}", idx).unwrap();
        idx += 1;
        rkyv_test_server::log_message("LOCAL LENDER", message_string);
        ticktimer.sleep_ms(500).unwrap();
    }
}

fn main() -> ! {
    log_server::init_wait().unwrap();

    log::info!(
        "Hello, world! This is the server, PID {}",
        xous::current_pid().unwrap()
    );
    let xns = xous_names::XousNames::new().unwrap();
    let sid = xns.register_name(api::SERVER_NAME).unwrap();

    let mut logstring_callback_connections = [None; 32];

    xous::create_thread_0(local_lender);

    loop {
        let mut msg = xous::receive_message(sid).unwrap();
        match FromPrimitive::from_usize(msg.body.id()) {
            Some(api::Opcode::Mathematics) => {
                handle_math_withcopy(msg.body.memory_message_mut().unwrap())
            }
            Some(api::Opcode::DoubleString) => {
                double_string(msg.body.memory_message_mut().unwrap())
            }
            Some(api::Opcode::LogString) => {
                let memory = msg.body.memory_message().unwrap();
                // If a callback exists, first pass this message to the callback server.
                for callback_conn in logstring_callback_connections.iter() {
                    if let Some(callback_sid) = callback_conn {
                        let buffer = unsafe { Buffer::from_memory_message(memory) };
                        buffer
                            .lend(
                                *callback_sid,
                                api::CallbackType::LogString.to_u32().unwrap(),
                            )
                            .unwrap();
                    }
                }
                handle_log_string(msg.body.memory_message().unwrap())
            }
            Some(api::Opcode::AddLogStringCallback) => {
                // The Log String Callback provides us a SID. Connect to that SID
                // and add it to the list of connections available.
                if let xous::Message::Scalar(xous::ScalarMessage {
                    id: _id,
                    arg1,
                    arg2,
                    arg3,
                    arg4,
                }) = msg.body
                {
                    let sid = xous::SID::from_u32(arg1 as _, arg2 as _, arg3 as _, arg4 as _);
                    let cb_conn = Some(xous::connect(sid).unwrap());
                    // Add this callback connection to the list of callbacks. If `AddLogStringCallback`
                    // is called multiple times, then it will receive multiple callbacks.
                    for entry in logstring_callback_connections.iter_mut() {
                        if *entry == None {
                            *entry = cb_conn;
                            break;
                        }
                    }
                }
            }
            None => break,
        }
    }
    // clean up our program
    log::trace!("main loop exit, destroying servers");
    xns.unregister_server(sid).unwrap();
    xous::destroy_server(sid).unwrap();
    log::trace!("quitting");
    xous::terminate_process(0)
}
