extern crate libloading;

use gpui::{
    AppContext, Bounds, Context, Corner, FocusHandle, InteractiveElement, IntoElement, KeyBinding,
    ParentElement, Render, Styled, Timer, Window, actions, black, canvas, div, fill, point, px,
    red, size,
};
use libloading::Library;
use libretro_sys::{
    API_VERSION, CoreAPI, ENVIRONMENT_GET_CAN_DUPE, ENVIRONMENT_SET_PIXEL_FORMAT, GameInfo,
};
use std::{
    ffi::{CString, c_void},
    fs, ptr,
    time::Duration,
};

const WIDTH: f32 = 640.;
const HEIGHT: f32 = 480.;

pub struct LibretroCore {
    _library: Library, // Keep library alive
    api: CoreAPI,
}

impl LibretroCore {
    fn new() -> Self {
        unsafe {
            let library = Library::new(
                "C:\\Users\\jorge\\Documents\\Projects\\thrustr\\gambatte_libretro.dll",
            )
            .expect("Failed to load core");

            let api = CoreAPI {
                retro_set_environment: *(library.get(b"retro_set_environment").unwrap()),
                retro_set_video_refresh: *(library.get(b"retro_set_video_refresh").unwrap()),
                retro_set_audio_sample: *(library.get(b"retro_set_audio_sample").unwrap()),
                retro_set_audio_sample_batch: *(library
                    .get(b"retro_set_audio_sample_batch")
                    .unwrap()),
                retro_set_input_poll: *(library.get(b"retro_set_input_poll").unwrap()),
                retro_set_input_state: *(library.get(b"retro_set_input_state").unwrap()),

                retro_init: *(library.get(b"retro_init").unwrap()),
                retro_deinit: *(library.get(b"retro_deinit").unwrap()),

                retro_api_version: *(library.get(b"retro_api_version").unwrap()),

                retro_get_system_info: *(library.get(b"retro_get_system_info").unwrap()),
                retro_get_system_av_info: *(library.get(b"retro_get_system_av_info").unwrap()),
                retro_set_controller_port_device: *(library
                    .get(b"retro_set_controller_port_device")
                    .unwrap()),

                retro_reset: *(library.get(b"retro_reset").unwrap()),
                retro_run: *(library.get(b"retro_run").unwrap()),

                retro_serialize_size: *(library.get(b"retro_serialize_size").unwrap()),
                retro_serialize: *(library.get(b"retro_serialize").unwrap()),
                retro_unserialize: *(library.get(b"retro_unserialize").unwrap()),

                retro_cheat_reset: *(library.get(b"retro_cheat_reset").unwrap()),
                retro_cheat_set: *(library.get(b"retro_cheat_set").unwrap()),

                retro_load_game: *(library.get(b"retro_load_game").unwrap()),
                retro_load_game_special: *(library.get(b"retro_load_game_special").unwrap()),
                retro_unload_game: *(library.get(b"retro_unload_game").unwrap()),

                retro_get_region: *(library.get(b"retro_get_region").unwrap()),
                retro_get_memory_data: *(library.get(b"retro_get_memory_data").unwrap()),
                retro_get_memory_size: *(library.get(b"retro_get_memory_size").unwrap()),
            };

            // Validate API version
            let api_version = (api.retro_api_version)();
            println!("API Version: {}", api_version);
            if api_version != API_VERSION {
                panic!(
                    "The Core has been compiled with a LibRetro API that is unexpected, we expected version to be: {} but it was: {}",
                    API_VERSION, api_version
                );
            }

            (api.retro_set_environment)(libretro_environment_callback);
            (api.retro_set_video_refresh)(libretro_set_video_refresh_callback);
            (api.retro_set_input_poll)(libretro_set_input_poll_callback);
            (api.retro_set_input_state)(libretro_set_input_state_callback);
            (api.retro_set_audio_sample)(libretro_set_audio_sample_callback);
            (api.retro_set_audio_sample_batch)(libretro_set_audio_sample_batch_callback);

            Self {
                _library: library,
                api,
            }
        }
    }

    pub unsafe fn load_rom_file(&self, rom_name: String) -> bool {
        let rom_name_cstring = CString::new(rom_name.clone()).expect("Failed to create CString");
        let rom_name_cptr = rom_name_cstring.as_ptr();
        let contents = fs::read(rom_name).expect("Failed to read file");
        let data: *const c_void = contents.as_ptr() as *const c_void;
        let game_info = GameInfo {
            path: rom_name_cptr,
            data,
            size: contents.len(),
            meta: ptr::null(),
        };
        let was_load_successful = unsafe { (self.api.retro_load_game)(&game_info) };
        if !was_load_successful {
            panic!("Rom Load was not successful");
        }
        was_load_successful
    }
}

pub struct Root {
    core: LibretroCore,
}

impl Root {
    pub fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        let core = LibretroCore::new();
        unsafe {
            (core.api.retro_init)();
            core.load_rom_file("C:\\Users\\jorge\\Documents\\Projects\\thrustr\\Pokemon - Red Version (USA, Europe) (SGB Enhanced).gb".to_string());
            (core.api.retro_run)();
        }

        Self { core }
    }
}

impl Render for Root {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        unsafe {
            ((self.core.api.retro_run)());
            window.request_animation_frame();
        }

        div()
            .w(px(WIDTH))
            .h(px(HEIGHT))
            .bg(black())
            .child(canvas(|_, _, _| {}, move |_, _, _window, _| {}))
    }
}

pub type EnvironmentCallback =
    unsafe extern "C" fn(command: libc::c_uint, data: *mut libc::c_void) -> bool;

unsafe extern "C" fn libretro_environment_callback(command: u32, return_data: *mut c_void) -> bool {
    match command {
        ENVIRONMENT_GET_CAN_DUPE => {
            unsafe { *(return_data as *mut bool) = true }; // Set the return_data to the value true
            println!("Set ENVIRONMENT_GET_CAN_DUPE to true");
        }
        ENVIRONMENT_SET_PIXEL_FORMAT => {
            println!(
                "TODO: Handle ENVIRONMENT_SET_PIXEL_FORMAT when we start drawing the the screen buffer"
            );
            return true;
        }
        _ => println!(
            "libretro_environment_callback Called with command: {}",
            command
        ),
    }
    false
}

unsafe extern "C" fn libretro_set_video_refresh_callback(
    frame_buffer_data: *const libc::c_void,
    width: libc::c_uint,
    height: libc::c_uint,
    pitch: libc::size_t,
) {
    if frame_buffer_data.is_null() {
        println!("frame_buffer_data was null");
        return;
    }
    println!(
        "libretro_set_video_refresh_callback, width: {}, height: {}, pitch: {}",
        width, height, pitch
    );
    let length_of_frame_buffer = width * height;
    let slice = unsafe {
        std::slice::from_raw_parts(
            frame_buffer_data as *const u8,
            length_of_frame_buffer as usize,
        )
    };
    println!("Frame Buffer: {:?}", slice);
}

unsafe extern "C" fn libretro_set_input_poll_callback() {
    println!("libretro_set_input_poll_callback")
}

unsafe extern "C" fn libretro_set_input_state_callback(
    port: libc::c_uint,
    device: libc::c_uint,
    index: libc::c_uint,
    id: libc::c_uint,
) -> i16 {
    println!("libretro_set_input_state_callback");
    return 0; // Hard coded 0 for now means nothing is pressed
}

unsafe extern "C" fn libretro_set_audio_sample_callback(left: i16, right: i16) {
    println!("libretro_set_audio_sample_callback");
}

unsafe extern "C" fn libretro_set_audio_sample_batch_callback(
    data: *const i16,
    frames: libc::size_t,
) -> libc::size_t {
    println!("libretro_set_audio_sample_batch_callback");
    return 1;
}
