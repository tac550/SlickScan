use std::{ffi::CString, sync::{Arc, Mutex}, thread::{JoinHandle, self}};

use eframe::{egui::{self, Response, Image, Context}, epaint::{Color32, ColorImage, TextureHandle}};
use sane_scan::{self, Sane, Device, DeviceHandle, DeviceOption, DeviceOptionValue, ValueType, OptionCapability, Frame};

fn main() {
    env_logger::init();

    let options = eframe::NativeOptions::default();

    // Initialize SANE components
    let version_code = 0;
    let sane_instance = Sane::init(version_code);

    match sane_instance {
        Ok(sane_instance) => eframe::run_native(
            "Roboarchive",
            options,
            Box::new(|cc| Box::new(RoboarchiveApp::new(cc, sane_instance)))),
        Err(error) => println!("Error occurred setting up SANE scanner interface: {}", error),
    }
}

struct ThDeviceHandle {
    handle: DeviceHandle,
}

unsafe impl Send for ThDeviceHandle {}

struct RoboarchiveApp {
    // SANE backend objects
    scanner_list: Vec<Device>,
    selected_scanner: usize,
    prev_selected_scanner: Option<usize>,
    selected_handle: Option<Arc<Mutex<ThDeviceHandle>>>,
    config_options: Vec<EditingDeviceOption>,
    sane_instance: Sane,

    // UI state controls
    ui_context: Arc<Mutex<Context>>,
    show_config: bool,
    scan_running: bool,

    // Threading resources
    texture_handles: Arc<Mutex<Vec<TextureHandle>>>,
    image_queue: Arc<Mutex<Vec<Option<Image>>>>,
    thread_handle: Option<JoinHandle<()>>,
}

impl RoboarchiveApp {
    fn new(cc: &eframe::CreationContext<'_>, sane_instance: Sane) -> Self {
        // Customize egui here with cc.egui_ctx.set_fonts and cc.egui_ctx.set_visuals.
        // Restore app state using cc.storage (requires the "persistence" feature).
        // Use the cc.gl (a glow::Context) to create graphics shaders and buffers that you can use
        // for e.g. egui::PaintCallback.
        Self {
            scanner_list: Default::default(),
            selected_scanner: Default::default(),
            prev_selected_scanner: Default::default(),
            selected_handle: Default::default(),
            config_options: Default::default(),
            sane_instance,
            ui_context: Arc::new(Mutex::new(cc.egui_ctx.clone())),
            show_config: Default::default(),
            scan_running: Default::default(),
            texture_handles: Default::default(),
            image_queue: Default::default(),
            thread_handle: Default::default(),
        }
    }

    fn refresh_devices(&mut self) {
        self.scanner_list = match self.sane_instance.get_devices(false) {
            Ok(devices) => devices,
            Err(error) => {
                println!("Error refreshing device list: {}", error);
                vec![]
            },
        };
        self.open_selected_device();
    }

    fn open_selected_device(&mut self) {
        // Don't open scanner if same scanner was already selected (if there was a previous scanner)
        if let Some(prev) = self.prev_selected_scanner {
            if prev == self.selected_scanner {
                return;
            }
        }

        // Open new scanner, updating previous field and closing configuration panel
        self.prev_selected_scanner = Some(self.selected_scanner);
        self.show_config = false;

        if let Some(device) = self.scanner_list.get(self.selected_scanner) {
            println!("Opening device {}", cstring_to_string(&device.name, "device name"));
            self.selected_handle = match device.open() {
                Ok(handle) => Some(Arc::new(Mutex::new(ThDeviceHandle { handle }))),
                Err(error) => {
                    println!("Failed to open device: {}", error);
                    None
                },
            };
        }
    }

    fn load_device_options(&mut self) {
        self.config_options.clear();

        if let Some(handle) = &self.selected_handle {
            let device_options = match handle.lock().unwrap().handle.get_options() {
                Ok(options) => options,
                Err(error) => {
                    println!("Failed to retrieve options: {}", error);
                    vec![]
                },
            };
        
            for option in device_options {
                let option_value = match option.type_ {
                    ValueType::Button => DeviceOptionValue::Button,
                    ValueType::Group => DeviceOptionValue::Group,
                    _ => {
                        match handle.lock().unwrap().handle.get_option(&option) {
                            Ok(opt) => opt,
                            Err(error) => DeviceOptionValue::String(string_to_cstring("ERROR: ".to_owned() + &error.to_string())),
                        }
                    },
                };
                self.config_options.push(EditingDeviceOption::new(option, option_value));
            }
        }
    }

    fn apply_config_changes(&mut self) {
        if let Some(handle) = &self.selected_handle {
            for option in self.config_options.iter_mut() {
                if !option.is_edited {
                    continue;
                }

                if let EditingDeviceOptionValue::Button = option.editing_value {
                    if let Err(error) = handle.lock().unwrap().handle.set_option_auto(&option.base_option) {
                        println!("Error applying configuration: {}", error);
                    }
                } else if let Ok(opt_val) = TryInto::<DeviceOptionValue>::try_into(&option.editing_value) {
                    if let Err(error) = handle.lock().unwrap().handle.set_option(&option.base_option, opt_val) {
                        println!("Error applying configuration: {}", error);
                    }
                } else {
                    println!("Error converting from editor value");
                }
            }

            self.load_device_options();
        } else {
            println!("Error: Not attached to a device handle!");
        }
    }

    fn start_scan(&mut self) {
        if let Some(handle) = self.selected_handle.as_mut() {
            self.scan_running = true;
            if let Err(error) = handle.lock().unwrap().handle.start_scan() {
                println!("Error occurred initiating scan: {}", error);
                self.scan_running = false;
                return;
            }

            self.start_reading_thread();
        }
    }

    fn start_reading_thread(&mut self) {
        if let Some(handle) = &self.selected_handle {
            let handle = handle.clone();
            let texture_buf = self.texture_handles.clone();
            let queue = self.image_queue.clone();
            let ctx = self.ui_context.clone();
            self.thread_handle = Some(thread::spawn(move || {
                let mut queue_index: usize = 0;
                texture_buf.lock().unwrap().clear();
                queue.lock().unwrap().clear();

                loop {
                    let (bytes_per_line, lines, format) = match handle.lock().unwrap().handle.get_parameters() {
                        Ok(params) => (
                            TryInto::<usize>::try_into(params.bytes_per_line).expect("Failed to convert `bytes_per_line` to unsigned"),
                            TryInto::<usize>::try_into(params.lines).expect("Failed to convert `lines` to unsigned"),
                            params.format),
                        Err(error) => {
                            println!("Error retrieving scan parameters: {}", error);
                            return
                        },
                    };

                    let result = match handle.lock().unwrap().handle.read_to_vec() {
                        Ok(image) => image,
                        Err(error) => {
                            println!("Error reading image data: {}", error);
                            return;
                        },
                    };

                    let pixels_per_line = match format {
                        Frame::Rgb => bytes_per_line / 3,
                        _ => bytes_per_line,
                    };

                    let result = match format {
                        Frame::Rgb => result,
                        _ => repeat_all_elements(result, 3),
                    };

                    let result = insert_after_every(result, 3, 255);

                    let image = ColorImage::from_rgba_unmultiplied([pixels_per_line, lines], &result);
                    texture_buf.lock().unwrap().push(ctx.lock().unwrap().load_texture(queue_index.to_string(), image, egui::TextureFilter::Linear));

                    let tex_handle = texture_buf.lock().unwrap().last().unwrap().clone();

                    queue.lock().unwrap().push(Some(egui::Image::new(&tex_handle, tex_handle.size_vec2())));

                    ctx.lock().unwrap().request_repaint();

                    queue_index += 1;
                    if handle.lock().unwrap().handle.start_scan().is_err() || queue_index > 20 {
                        break;
                    }
                }
            }));
        }
    }
    fn stop_reading_thread(&mut self) {
        if let Some(handle) = self.thread_handle.take() {
            if let Err(error) = handle.join() {
                println!("Error occurred when stopping scan: {:?}", error);
            }
        }
    }

    fn cancel_scan(&mut self) {
        self.scan_running = false;

        self.stop_reading_thread();
    }
}

impl eframe::App for RoboarchiveApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("MainUI-TopPanel").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                
                // Refresh button
                if ui.button("↻").on_hover_text_at_pointer("Refresh the device list").clicked() {
                    self.refresh_devices();
                };

                ui.add_enabled_ui(!self.scanner_list.is_empty(), |ui| {
                    // Scanner selection dropdown
                    if egui::ComboBox::from_label(" is the selected scanner.")
                        .show_index(ui, &mut self.selected_scanner, self.scanner_list.len(),
                        |i| match self.scanner_list.get(i) {
                            Some(device) => format!("{} {} — {}",
                                cstring_to_string(&device.name, "device name"),
                                cstring_to_string(&device.model, "device model"),
                                cstring_to_string(&device.vendor, "device vendor")),
                            None => String::from("(None)"),
                        })
                    .on_disabled_hover_text("No scanner available—try clicking refresh.")
                    .changed() {
                        self.open_selected_device();
                    };
                });

                ui.add_enabled_ui(self.selected_handle.is_some() && !self.scan_running, |ui| {
                    // Scanner configuration dialog button
                    if ui.button("Configure scanner...").clicked() {
                        self.show_config = true;

                        self.load_device_options();
                    }

                    if ui.button("Start scanning").clicked() {
                        self.start_scan();
                    }
                });

                ui.add_enabled_ui(self.selected_handle.is_some() && self.scan_running, |ui| {
                    if ui.button("Cancel scan").clicked() {
                        self.cancel_scan();
                    }
                })
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for image in self.image_queue.lock().unwrap().iter().flatten() {
                        ui.add(*image);
                    }
                });
            });
        });

        if self.show_config {
            egui::Window::new("Scanner Configuration").default_size([680.0, 500.0]).show(ctx, |ui| {
                egui::TopBottomPanel::bottom("close_panel")
                .resizable(false)
                .show_inside(ui, |ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Close").clicked() {
                            self.show_config = false;
                        }

                        if ui.button("Apply").clicked() {
                            self.apply_config_changes();
                        }
                    });
                });

                egui::CentralPanel::default().show_inside(ui, |ui| {
                    egui::ScrollArea::both().show(ui, |ui| {
                        egui::Grid::new("device_config").striped(true).max_col_width(160.0).show(ui, |ui| {
                            for option in self.config_options.iter_mut() {

                                if let ValueType::Group = option.base_option.type_ {
                                    // Group titles get a special label and no controls (column 1)
                                    ui.colored_label(Color32::LIGHT_BLUE,
                                        cstring_to_string(&option.base_option.title, "group title"));
                                } else {
                                    // Draw the option item's label (column 1)
                                    let option_title = cstring_to_string(&option.base_option.title, "option title");
                                    ui.label(option_title).on_hover_text(cstring_to_string(&option.base_option.desc, "option description"));
                                }

                                // Draw the option value controls (column 2)
                                ui.add_enabled_ui(option.base_option.cap.contains(OptionCapability::SOFT_SELECT), |ui| {
                                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                        render_device_option_controls(ui, option);
                                    }).response.on_disabled_hover_text("This option cannot be changed in software — look on the hardware device to adjust.");
                                });

                                ui.end_row();
                            }
                        });
                    });
                });
            });
        }
    }
}

fn cstring_to_string(cstring: &CString, data_type: &str) -> String {
    cstring.clone().into_string().unwrap_or(format!("Error reading {}!", data_type))
}

fn string_to_cstring(string: String) -> CString {
    CString::new(string).unwrap_or_default()
}

fn render_device_option_controls(ui: &mut egui::Ui, option: &mut EditingDeviceOption) {
    if option.base_option.cap.contains(OptionCapability::INACTIVE) {
        ui.colored_label(Color32::DARK_RED, "(Inactive)").on_hover_text("This option is inactive. There may be another option that, once applied, causes this option to take effect.");
        return;
    }

    match &mut option.editing_value {
        EditingDeviceOptionValue::Bool(val) => option_edited_if_changed(ui.checkbox(val, ""), option),
        EditingDeviceOptionValue::Int(val) => {
            match &option.base_option.constraint {
                sane_scan::OptionConstraint::WordList(list) => {
                    if egui::ComboBox::from_id_source(option.base_option.option_idx).selected_text(val.to_owned()).show_ui(ui, |ui| {
                        for word in list {
                            ui.selectable_value(val, word.to_string(), word.to_string());
                        }
                    }).response.clicked() {
                        option.is_edited = true;
                    }
                },
                sane_scan::OptionConstraint::Range { range, quant } => {
                    ui.colored_label(Color32::GOLD, format!("(Range: {} – {}, step: {})", range.start, range.end, quant));
                    option_edited_if_changed(ui.text_edit_singleline( val), option);
                },
                _ => option_edited_if_changed(ui.text_edit_singleline( val), option),
            }
        },
        EditingDeviceOptionValue::Fixed(val) => {
            match &option.base_option.constraint {
                sane_scan::OptionConstraint::Range { range, quant } => {
                    ui.colored_label(Color32::GOLD, format!("(Range: {} – {}, step: {})",
                        sane_fixed_to_float(range.start), sane_fixed_to_float(range.end), sane_fixed_to_float(*quant)));
                    option_edited_if_changed(ui.text_edit_singleline(val), option);
                },
                _ => option_edited_if_changed(ui.text_edit_singleline(val), option),
            }
        },
        EditingDeviceOptionValue::String(val) => {
            match &option.base_option.constraint {
                sane_scan::OptionConstraint::StringList(list) => {
                    let string_list: Vec<String> = list.iter().map(|item| cstring_to_string(item, "option choice")).collect();
                    if egui::ComboBox::from_id_source(option.base_option.option_idx).selected_text(val.to_owned()).show_ui(ui, |ui| {
                        for string in string_list {
                            ui.selectable_value(val, string.clone(), string);
                        }
                    }).response.clicked() {
                        option.is_edited = true;
                    }
                },
                _ => option_edited_if_changed(ui.text_edit_singleline(val), option),
            }
        },
        EditingDeviceOptionValue::Button => {
            if ui.button("Activate").clicked() {
                option.is_edited = true;
            }
            if option.is_edited {
                ui.label("Will activate when Apply button is clicked.");
            }
        },
        EditingDeviceOptionValue::Group => return,
    }

    ui.add_enabled_ui(option.is_edited, |ui| {
        if ui.button("Reset").clicked() {
            option.reset_editor_value();
        }
    });
}

fn option_edited_if_changed(response: Response, option: &mut EditingDeviceOption) {
    if response.changed() {
        option.is_edited = true;
    }
}

fn insert_after_every<T: Clone>(ts: Vec<T>, after: usize, elem: T) -> Vec<T> {
    let mut result = Vec::new();
    for (i, e) in ts.into_iter().enumerate() {
        result.push(e);
        if (i + 1) % after == 0 {
            result.push(elem.clone());
        }
    }

    result
}

fn repeat_all_elements<T: Clone>(ts: Vec<T>, repeated: usize) -> Vec<T> {
    let mut result = Vec::new();
    for e in ts.into_iter() {
        for _ in 0..repeated {
            result.push(e.clone());
        }
    }

    result
}

#[derive(Debug)]
struct EditingDeviceOption {
    base_option: DeviceOption,
    editing_value: EditingDeviceOptionValue,
    is_edited: bool,
    original_value: DeviceOptionValue,
}

impl EditingDeviceOption {
    fn new(base_option: DeviceOption, original_value: DeviceOptionValue) -> Self {
        Self {
            base_option,
            editing_value: (&original_value).into(),
            is_edited: false,
            original_value,
        }
    }

    fn reset_editor_value(&mut self) {
        self.editing_value = (&self.original_value).into();
        self.is_edited = false;
    }
}

#[derive(Debug)]
enum EditingDeviceOptionValue {
	Bool(bool),
	Int(String),
	Fixed(String),
	String(String),
	Button,
	Group,
}

impl From<&DeviceOptionValue> for EditingDeviceOptionValue {
    fn from(opt_value: &DeviceOptionValue) -> Self {
        match opt_value {
            DeviceOptionValue::Bool(val) => Self::Bool(*val),
            DeviceOptionValue::Int(val) => Self::Int(val.to_string()),
            DeviceOptionValue::Fixed(val) => Self::Fixed(sane_fixed_to_float(*val).to_string()),
            DeviceOptionValue::String(val) => Self::String(cstring_to_string(val, "option value")),
            DeviceOptionValue::Button => Self::Button,
            DeviceOptionValue::Group => Self::Group,
        }
    }
}

impl TryFrom<&EditingDeviceOptionValue> for DeviceOptionValue {
    fn try_from(opt_edit: &EditingDeviceOptionValue) -> Result<Self, Self::Error> {
        match opt_edit {
            EditingDeviceOptionValue::Bool(val) => Ok(Self::Int((*val).into())),
            EditingDeviceOptionValue::Int(val) => Ok(Self::Int(val.parse()?)),
            EditingDeviceOptionValue::Fixed(val) => Ok(Self::Fixed(sane_float_to_fixed(val.parse()?))),
            EditingDeviceOptionValue::String(val) => Ok(Self::String(string_to_cstring(val.clone()))),
            EditingDeviceOptionValue::Button => Ok(Self::Button),
            EditingDeviceOptionValue::Group => Ok(Self::Group),
        }
    }

    type Error = Box<dyn std::error::Error>;
}

fn sane_fixed_to_float(fixed: i32) -> f64 {
    if fixed == std::i32::MIN {
        return -32768.0;
    }
    
    let mut c = fixed.abs();
    let mut sign = 1;

    if fixed < 0 {
        c = fixed - 1;
        c = !c;
        sign = -1;
    }

    ((1.0 * c as f64) / (2i32.pow(16)) as f64) * sign as f64
}

fn sane_float_to_fixed(fixed: f64) -> i32 {
    if fixed == -32768.0 {
        return i32::MIN;
    }

    let a = fixed * 2i32.pow(16) as f64;
    let mut b = a.round() as i32;

    if a < 0.0 {
        b = b.abs();
        b = !b;
        b += 1;
    }

    b
}