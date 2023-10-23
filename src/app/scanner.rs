use sane_scan::{DeviceHandle, DeviceOption, DeviceOptionValue};

use crate::util::{cstring_to_string, string_to_cstring, sane_fixed_to_float, float_to_sane_fixed};

pub struct ThDeviceHandle {
    pub handle: DeviceHandle,
}

unsafe impl Send for ThDeviceHandle {}

#[derive(Debug)]
pub struct EditingDeviceOption {
    pub base_option: DeviceOption,
    pub editing_value: EditingDeviceOptionValue,
    pub is_edited: bool,
    original_value: DeviceOptionValue,
}

impl EditingDeviceOption {
    pub fn new(base_option: DeviceOption, original_value: DeviceOptionValue) -> Self {
        Self {
            base_option,
            editing_value: (&original_value).into(),
            is_edited: false,
            original_value,
        }
    }

    pub fn reset_editor_value(&mut self) {
        self.editing_value = (&self.original_value).into();
        self.is_edited = false;
    }
}

#[derive(Debug)]
pub enum EditingDeviceOptionValue {
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