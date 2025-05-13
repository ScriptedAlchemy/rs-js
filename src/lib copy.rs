#![deny(clippy::all)]

use napi::{
  bindgen_prelude::*,
  Env, JsBoolean, JsFunction, JsObject, JsUnknown, Result, Task, ValueType,
  threadsafe_function::{
    ThreadsafeFunction,
    ErrorStrategy,
    ThreadsafeFunctionCallMode,
    ThreadSafeCallContext,
  },
  NapiRaw,
  NapiValue
};
use napi_derive::napi;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::rc::Rc;
use std::cell::RefCell;
use std::sync::Once;

// Configure Rayon's thread pool once on startup
static CONFIGURE_RAYON: Once = Once::new();

fn configure_rayon_pool() {
    CONFIGURE_RAYON.call_once(|| {
        // Configure Rayon to use more threads if available
        rayon::ThreadPoolBuilder::new()
            .num_threads(num_cpus::get())
            .build_global()
            .expect("Failed to build Rayon thread pool");
    });
}

#[napi(object)]
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct User {
  pub id: u32,
  #[serde(rename = "firstName")]
  pub first_name: String,
  pub age: u32,
  pub role: String,
}

// This struct is used to parse the top-level "users" key from data.json
#[derive(Deserialize)]
struct UserData {
  users: Vec<User>,
}

// --- Simplest possible synchronous filter - no Tasks, no async ---
#[napi]
pub fn filter_admins_sync(env: Env) -> Result<JsObject> {
  // Just read the file synchronously
  let path = Path::new("data.json");
  let file_content = fs::read_to_string(path)
    .map_err(|e| napi::Error::from_reason(format!("Failed to read data.json: {}", e)))?;
  
  // Parse the JSON
  let data: UserData = serde_json::from_str(&file_content)
    .map_err(|e| napi::Error::from_reason(format!("Failed to parse data.json: {}", e)))?;
  
  // Apply a simple filter - nothing fancy
  let admins_count = data.users
    .into_iter()
    .filter(|user| user.role == "admin")
    .count();
  
  // Return only the count instead of the filtered array
  let mut obj = env.create_object()?;
  obj.set_named_property("count", admins_count as u32)?;
  Ok(obj)
}

// --- filter_admins_simple implementation (Asynchronous using Rayon) ---
struct FilterAdminsSimpleTask {
  users: Vec<User>,
}

impl Task for FilterAdminsSimpleTask {
  type Output = u32;
  type JsValue = u32;

  fn compute(&mut self) -> Result<Self::Output> {
    // Configure Rayon
    configure_rayon_pool();
    
    // Use a simple parallel filter and return only the count
    let count = self.users
      .par_iter()
      .filter(|user| user.role == "admin")
      .count();
    
    Ok(count as u32)
  }

  fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
    Ok(output)
  }
}

#[napi(js_name = "filterAdminsSimple", ts_return_type = "Promise<number>")]
pub fn filter_admins_simple() -> AsyncTask<FilterAdminsSimpleTask> {
  // Load and parse the data.json file which now contains 50k entries
  let path = Path::new("data.json");
  let file_content = match fs::read_to_string(path) {
    Ok(content) => content,
    Err(e) => {
      eprintln!("Failed to read data.json: {}", e);
      return AsyncTask::new(FilterAdminsSimpleTask { users: Vec::new() });
    }
  };
  
  let data: UserData = match serde_json::from_str(&file_content) {
    Ok(data) => data,
    Err(e) => {
      eprintln!("Failed to parse data.json: {}", e);
      return AsyncTask::new(FilterAdminsSimpleTask { users: Vec::new() });
    }
  };
  
  AsyncTask::new(FilterAdminsSimpleTask { users: data.users })
}



// --- filter_users_with_callback_from_file (Asynchronous Task with JS callback & Rayon) ---
struct FilterUsersFromFileTask {
  tsfn: ThreadsafeFunction<User, ErrorStrategy::Fatal>,
  users_to_process: Vec<User>,
}

impl Task for FilterUsersFromFileTask {
  type Output = u32;
  type JsValue = u32;

  fn compute(&mut self) -> Result<Self::Output> {
    let mut count = 0;
    
    // Process each user sequentially instead of in parallel
    for user in &self.users_to_process {
      // Use block_on to wait for the callback result
      // We need to get a copy of the user for the callback
      let user_clone = user.clone();
      
      // Call the JS callback and get a bool return value
      let should_keep = self.tsfn.call(user_clone, ThreadsafeFunctionCallMode::Blocking) == napi::Status::Ok;
      
      // If the call was successful and the user role is "admin", increment count
      if should_keep && user.role == "admin" {
        count += 1;
      }
    }
    
    Ok(count)
  }

  fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
    Ok(output)
  }
}

#[napi(
  js_name = "filterUsersWithCallbackFromFile",
  ts_args_type = "callback: (user: User) => boolean",
  ts_return_type = "Promise<number>"
)]
pub fn filter_users_with_callback_from_file(
  _env: Env,
  callback: JsFunction,
) -> Result<AsyncTask<FilterUsersFromFileTask>> {
  // Load users from data.json - no need to duplicate as the file should already contain 50k entries
  let path = Path::new("data.json");
  let file_content = fs::read_to_string(path)
    .map_err(|e| napi::Error::from_reason(format!("Failed to read data.json: {}", e)))?;
  let data: UserData = serde_json::from_str(&file_content)
    .map_err(|e| napi::Error::from_reason(format!("Failed to parse data.json: {}", e)))?;

  // Switch to ErrorStrategy::Fatal which is simpler and doesn't use Result
  let tsfn: ThreadsafeFunction<User, ErrorStrategy::Fatal> = callback
    .create_threadsafe_function(
        0, // Max queue size
        |ctx: ThreadSafeCallContext<User>| { // Input: User struct from Rust
            ctx.env.to_js_value(&ctx.value).map(|v| vec![v])
        }
    )?;

  let task = FilterUsersFromFileTask {
    tsfn,
    users_to_process: data.users,
  };
  Ok(AsyncTask::new(task))
}
