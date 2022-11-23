use std::{ffi::CString, sync::{Arc, Mutex}, thread::{JoinHandle, self}, path::PathBuf, fs::{File, self}, io::BufWriter};

use eframe::{egui::{self, Response, Context, Sense}, epaint::{Color32, ColorImage, TextureHandle, Vec2}};
use printpdf::{PdfDocument, Mm, ImageXObject, Px, ColorSpace, ColorBits, Image, ImageTransform};
use sane_scan::{self, Sane, Device, DeviceHandle, DeviceOption, DeviceOptionValue, ValueType, OptionCapability, Frame};
use tinyfiledialogs::{select_folder_dialog, MessageBoxIcon, message_box_ok, message_box_yes_no, YesNo};

const DEFAULT_FILE_NAME: &str = "scan.pdf";
const ERR_DIALOG_TITLE: &str = "Roboarchive Error";
const LETTER_WIDTH_MM: f64 = 215.9;
const LETTER_HEIGHT_MM: f64 = 279.4;
const LETTER_WIDTH_IN: f64 = 8.5;
const LETTER_HEIGHT_IN: f64 = 11.0;

fn main() {
    env_logger::init();

    let options = eframe::NativeOptions {
        initial_window_size: Some(Vec2::new(1050.0, 850.0)),
        ..Default::default()
    };

    // Initialize SANE components
    let version_code = 0;
    let sane_instance = Sane::init(version_code);

    match sane_instance {
        Ok(sane_instance) => eframe::run_native(
            "Roboarchive",
            options,
            Box::new(|cc| Box::new(RoboarchiveApp::new(cc, sane_instance)))),
        Err(error) => message_box_ok(ERR_DIALOG_TITLE, &format!("Error occurred while setting up SANE scanner interface: {}", error), MessageBoxIcon::Error),
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
    search_network: bool,
    show_config: bool,
    scan_running: bool,
    image_max_x: f32,
    selecting_page: usize,

    scanned_images: Arc<Mutex<Vec<ScannedImage>>>,
    selected_page_indices: Vec<usize>,
    show_saved_images: bool,

    // UI Response references
    path_field: Option<Response>,

    // Threading resources
    scan_thread_handle: Option<JoinHandle<()>>,
    scan_cancelled: Arc<Mutex<bool>>,

    // I/O state information
    root_location: Option<PathBuf>,
    file_save_path: String,
}

impl RoboarchiveApp {
    fn new(cc: &eframe::CreationContext<'_>, sane_instance: Sane) -> Self {
        Self {
            scanner_list: Default::default(),
            selected_scanner: Default::default(),
            prev_selected_scanner: Default::default(),
            selected_handle: Default::default(),
            config_options: Default::default(),
            sane_instance,
            ui_context: Arc::new(Mutex::new(cc.egui_ctx.clone())),
            search_network: Default::default(),
            show_config: Default::default(),
            scan_running: Default::default(),
            image_max_x: 200.0,
            selecting_page: Default::default(),
            scanned_images: Default::default(),
            selected_page_indices: Default::default(),
            show_saved_images: Default::default(),
            path_field: Default::default(),
            scan_thread_handle: Default::default(),
            scan_cancelled: Default::default(),
            root_location: Default::default(),
            file_save_path: Default::default(),
        }
    }

    fn refresh_devices(&mut self) {
        self.scanner_list = match self.sane_instance.get_devices(!self.search_network) {
            Ok(devices) => devices,
            Err(error) => {
                message_box_ok(ERR_DIALOG_TITLE, &format!("Error refreshing device list: {}", error), MessageBoxIcon::Warning);
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
            self.selected_handle = match device.open() {
                Ok(handle) => Some(Arc::new(Mutex::new(ThDeviceHandle { handle }))),
                Err(error) => {
                    message_box_ok(ERR_DIALOG_TITLE, &format!("Failed to open device: {}", error), MessageBoxIcon::Error);
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
                    message_box_ok(ERR_DIALOG_TITLE, &format!("Failed to retrieve options: {}", error), MessageBoxIcon::Warning);
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
                        message_box_ok(ERR_DIALOG_TITLE, &format!("Error applying configuration: {}", error), MessageBoxIcon::Error);
                    }
                } else if let Ok(opt_val) = TryInto::<DeviceOptionValue>::try_into(&option.editing_value) {
                    if let Err(error) = handle.lock().unwrap().handle.set_option(&option.base_option, opt_val) {
                        message_box_ok(ERR_DIALOG_TITLE, &format!("Error applying configuration: {}", error), MessageBoxIcon::Error);
                    }
                } else {
                    message_box_ok(ERR_DIALOG_TITLE, "Error converting from editor value", MessageBoxIcon::Error);
                }
            }

            self.load_device_options();
        } else {
            message_box_ok(ERR_DIALOG_TITLE, "Not attached to a device handle!", MessageBoxIcon::Error);
        }
    }

    fn start_scan(&mut self) {
        if let Some(handle) = self.selected_handle.as_mut() {
            self.scan_running = true;
            if let Err(error) = handle.lock().unwrap().handle.start_scan() {
                message_box_ok(ERR_DIALOG_TITLE, &format!("Error occurred while initiating scan: {}", error), MessageBoxIcon::Error);
                self.scan_running = false;
                return;
            }

            *self.scan_cancelled.lock().unwrap() = false;
            self.start_reading_thread();
        }
    }

    fn start_reading_thread(&mut self) {
        if let Some(handle) = &self.selected_handle {
            let handle = handle.clone();
            let image_buf = self.scanned_images.clone();
            let ctx = self.ui_context.clone();
            let interrupt = self.scan_cancelled.clone();

            self.clear_selection();
            self.scan_thread_handle = Some(thread::spawn(move || {
                let mut queue_index: usize = 0;
                image_buf.lock().unwrap().clear();

                loop {
                    let scanned_pixels = match handle.lock().unwrap().handle.read_to_vec() {
                        Ok(image) => image,
                        Err(error) => {
                            message_box_ok(ERR_DIALOG_TITLE, &format!("Error reading image data: {}", error), MessageBoxIcon::Error);
                            return
                        },
                    };

                    let parameters = match handle.lock().unwrap().handle.get_parameters() {
                        Ok(params) => params,
                        Err(error) => {
                            message_box_ok(ERR_DIALOG_TITLE, &format!("Error retrieving scan parameters: {}", error), MessageBoxIcon::Error);
                            return
                        },
                    };

                    let bytes_per_line = TryInto::<usize>::try_into(parameters.bytes_per_line).expect("Failed to convert `bytes_per_line` to unsigned");
                    let lines = scanned_pixels.len() / bytes_per_line;

                    let pixels_per_line = match parameters.format {
                        Frame::Rgb => bytes_per_line / 3,
                        _ => bytes_per_line,
                    };

                    let pixels = match parameters.format {
                        Frame::Rgb => scanned_pixels,
                        _ => repeat_all_elements(scanned_pixels, 3),
                    };

                    let pixels_with_alpha = insert_after_every(pixels.clone(), 3, 255);

                    let image = ColorImage::from_rgba_unmultiplied([pixels_per_line, lines], &pixels_with_alpha);

                    let scanned_image = ScannedImage {
                        pixels,
                        texture_handle: ctx.lock().unwrap().load_texture(queue_index.to_string(), image, egui::TextureFilter::Linear),
                        selected_as_page: None,
                        saved_to_file: false,
                    };

                    image_buf.lock().unwrap().push(scanned_image);

                    ctx.lock().unwrap().request_repaint();

                    queue_index += 1;
                    if *interrupt.lock().unwrap() || handle.lock().unwrap().handle.start_scan().is_err() {
                        break;
                    }
                }
            }));
        }
    }
    fn stop_reading_thread(&mut self) {
        *self.scan_cancelled.lock().unwrap() = true;
        if let Some(handle) = self.scan_thread_handle.take() {
            if let Err(error) = handle.join() {
                message_box_ok(ERR_DIALOG_TITLE, "Error occurred while stopping scan (see console for details)", MessageBoxIcon::Error);
                println!("Error occurred while stopping scan: {:?}", error);
            }
        }
    }

    fn cancel_scan(&mut self) {
        self.stop_reading_thread();
        self.scan_running = false;
    }

    fn clear_selection_from(&mut self, index: usize) {
        for n in (index..self.selected_page_indices.len()).rev() {
            self.scanned_images.lock().unwrap()[self.selected_page_indices[n]]
                .selected_as_page = None;
            self.selected_page_indices.pop();
        }

        self.selecting_page = index;
    }

    fn clear_selection(&mut self) {
        self.clear_selection_from(0);
    }

    fn mark_selection_saved(&mut self) {
        for n in (0..self.selected_page_indices.len()).rev() {
            self.scanned_images.lock().unwrap()[self.selected_page_indices[n]]
                .saved_to_file = true;
        }
    }

    fn write_pdf(&mut self) -> Result<SaveStatus, Box<dyn std::error::Error>> {
        if self.selected_page_indices.is_empty() {
            return Err("No pages selected".to_owned().into());
        }

        if let Some(root_path) = &self.root_location {
            let file_path = if self.file_save_path.trim().is_empty() { DEFAULT_FILE_NAME } else { &self.file_save_path };
            let saving_path = root_path.join(file_path).with_extension("pdf");

            if let Some(p) = saving_path.parent() {
                if !p.exists() {
                    if let YesNo::No = message_box_yes_no("Create directory?", &format!("The location {} does not exist. Create it?", p.to_string_lossy()), MessageBoxIcon::Question, YesNo::Yes) {
                        return Ok(SaveStatus::Cancelled);
                    }
                    fs::create_dir_all(p)?
                }
            };

            if saving_path.exists() {
                if let YesNo::No = message_box_yes_no("Overwrite file?", "A file with that name already exists. Overwrite?", MessageBoxIcon::Question, YesNo::No) {
                    return Ok(SaveStatus::Cancelled);
                }
            }

            let doc = PdfDocument::empty("");

            for i in &self.selected_page_indices {
                let (new_page, new_layer) = doc.add_page(Mm(LETTER_WIDTH_MM), Mm(LETTER_HEIGHT_MM), "Layer 1");
                let current_layer = doc.get_page(new_page).get_layer(new_layer);
    
                let images_mutex = self.scanned_images.lock().unwrap();
                let scanned_image = images_mutex.get(*i).ok_or("Page index exceeded size of image vector")?;
    
                let image = Image::from(ImageXObject {
                    width: Px(scanned_image.texture_handle.size()[0]),
                    height: Px(scanned_image.texture_handle.size()[1]),
                    color_space: ColorSpace::Rgb,
                    bits_per_component: ColorBits::Bit8,
                    interpolate: true,
                    image_data: scanned_image.pixels.clone(),
                    image_filter: None,
                    clipping_bbox: None,
                });
    
                let inches_unscaled_x = scanned_image.texture_handle.size()[0] as f64 / 300.0;
                let inches_unscaled_y = scanned_image.texture_handle.size()[1] as f64 / 300.0;
    
                let scale_factor_x = LETTER_WIDTH_IN / inches_unscaled_x;
                let scale_factor_y = LETTER_HEIGHT_IN / inches_unscaled_y;
    
                image.add_to_layer(current_layer, ImageTransform {
                    translate_x: None,
                    translate_y: None,
                    rotate: None,
                    scale_x: Some(scale_factor_x),
                    scale_y: Some(scale_factor_y),
                    dpi: None,
                });
            }

            doc.save(&mut BufWriter::new(File::create(saving_path)?))?;

            Ok(SaveStatus::Completed)
        } else {
            Err("No root save location selected".to_owned().into())
        }
    }
}

impl eframe::App for RoboarchiveApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {

        if ctx.input().key_pressed(egui::Key::Escape) {
            self.clear_selection();
        }

        egui::TopBottomPanel::top("MainUI-TopPanel").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui.button("↻").on_hover_text_at_pointer("Refresh the device list").clicked() {
                    self.refresh_devices();
                };

                ui.checkbox(&mut self.search_network, "Search the network for devices");

                ui.add_enabled_ui(!self.scanner_list.is_empty(), |ui| {
                    if egui::ComboBox::from_label(" is the selected scanner.")
                        .show_index(ui, &mut self.selected_scanner, self.scanner_list.len(),
                        |i| match self.scanner_list.get(i) {
                            Some(device) => format!("{} — {}",
                                cstring_to_string(&device.name, "device name"),
                                cstring_to_string(&device.model, "device model")),
                            None => String::from("(None)"),
                        })
                    .on_disabled_hover_text("No scanner available—try clicking refresh")
                    .changed() {
                        self.open_selected_device();
                    };
                });

                ui.add_enabled_ui(self.selected_handle.is_some() && !self.scan_running, |ui| {
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

        egui::TopBottomPanel::bottom("MainUI-BottomPanel").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.add(egui::Slider::new(&mut self.image_max_x, 100.0..=500.0).text("Preview size"));

                if ui.button("Select root save location...").clicked() {
                    if let Some(path) = select_folder_dialog("Select root save location", self.root_location.as_ref().unwrap_or(&PathBuf::new()).to_str().unwrap_or("")) {
                        self.root_location = Some(PathBuf::from(path));
                    }
                }

                if let Some(path) = &self.root_location {
                    ui.colored_label(Color32::GREEN, (*path.canonicalize().unwrap_or_default().to_string_lossy()).to_owned() + std::path::MAIN_SEPARATOR.to_string().as_str());
                } else {
                    ui.colored_label(Color32::RED, "No save location selected");
                }

                ui.label("File name/path: ");

                self.path_field = Some(ui.add(egui::TextEdit::singleline(&mut self.file_save_path).hint_text(DEFAULT_FILE_NAME).cursor_at_end(false)));

                if let Some(field) = &self.path_field {
                    if field.lost_focus() && ctx.input().key_pressed(egui::Key::Enter) {
                        match self.write_pdf() {
                            Ok(status) => if let SaveStatus::Completed = status {
                                self.mark_selection_saved();
                                self.clear_selection();
                            },
                            Err(error) =>
                                message_box_ok(ERR_DIALOG_TITLE, &format!("Error occurred while saving PDF file: {}", error), MessageBoxIcon::Warning),
                        }
                    }
                }

                ui.checkbox(&mut self.show_saved_images, "Show saved")
                    .on_hover_text("Show scanned images even after they are saved to a file (selecting reveals previously-saved images)");
            });
        });

        let mut clearing_from_index: Option<usize> = None;

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (i, image) in self.scanned_images.lock().unwrap().iter_mut().enumerate() {
                        if image.saved_to_file && !self.show_saved_images {
                            continue;
                        }
                        
                        if ui.add(egui::Image::new(&image.texture_handle, scale_image_size(image.texture_handle.size_vec2(), self.image_max_x))
                            .tint(if let Some(n) = image.selected_as_page {selection_tint_color(n)} else {Color32::WHITE})
                            .sense(Sense::click()))
                                .on_hover_text_at_pointer(if let Some(page) = image.selected_as_page {format!("Page {}", page+1)} else {format!("Selecting page {}...", self.selecting_page+1)})
                                .clicked() {
                                    match image.selected_as_page {
                                        Some(idx) => {
                                            clearing_from_index = Some(idx);
                                        },
                                        None => {
                                            self.selected_page_indices.push(i);
                                            image.selected_as_page = Some(self.selecting_page);
                                            self.selecting_page += 1;    
                                        },
                                    }
                                    
                                    if let Some(resp) = &self.path_field {
                                        resp.request_focus();
                                    }
                        };
                    }
                });
            });
        });

        if let Some(idx) = clearing_from_index {
            self.clear_selection_from(idx);
        }

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

fn selection_tint_color(page_i: usize) -> Color32 {
    Color32::from_rgba_premultiplied(255 - ((page_i+1) * 50) as u8, 255 - ((page_i+1) * 50) as u8, 255, 50)
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

fn scale_image_size(original: Vec2, max_x: f32) -> Vec2 {
    let factor = max_x / original.x;
    original * factor
}

struct ScannedImage {
    pixels: Vec<u8>,
    texture_handle: TextureHandle,
    selected_as_page: Option<usize>,
    saved_to_file: bool,
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
            EditingDeviceOptionValue::Fixed(val) => Ok(Self::Fixed(float_to_sane_fixed(val.parse()?))),
            EditingDeviceOptionValue::String(val) => Ok(Self::String(string_to_cstring(val.clone()))),
            EditingDeviceOptionValue::Button => Ok(Self::Button),
            EditingDeviceOptionValue::Group => Ok(Self::Group),
        }
    }

    type Error = Box<dyn std::error::Error>;
}

enum SaveStatus {
    Completed,
    Cancelled,
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

fn float_to_sane_fixed(fixed: f64) -> i32 {
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