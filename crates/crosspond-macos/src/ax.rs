//! Accessibility attribute reads and UI actions.
//!
//! `unsafe` stays here: these are C APIs from the ApplicationServices framework.

use std::ptr;

use core_foundation::array::CFArray;
use core_foundation::base::{CFType, CFTypeRef, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::CFDictionary;
use core_foundation::string::{CFString, CFStringRef};

const AX_SUCCESS: i32 = 0;

type AxUiElementRef = *const std::ffi::c_void;
type CfArrayRef = *const std::ffi::c_void;

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    static kAXTrustedCheckOptionPrompt: CFStringRef;
    fn AXIsProcessTrusted() -> bool;
    fn AXIsProcessTrustedWithOptions(options: core_foundation::dictionary::CFDictionaryRef)
    -> bool;
    fn AXUIElementCreateSystemWide() -> AxUiElementRef;
    fn AXUIElementCreateApplication(pid: i32) -> AxUiElementRef;
    fn AXUIElementCopyAttributeValue(
        element: AxUiElementRef,
        attribute: CFStringRef,
        value: *mut CFTypeRef,
    ) -> i32;
    fn AXUIElementCopyActionNames(element: AxUiElementRef, names: *mut CfArrayRef) -> i32;
    fn AXUIElementPerformAction(element: AxUiElementRef, action: CFStringRef) -> i32;
    fn AXUIElementSetAttributeValue(
        element: AxUiElementRef,
        attribute: CFStringRef,
        value: CFTypeRef,
    ) -> i32;
}

pub fn prompt_and_is_trusted() -> bool {
    let key = unsafe { CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt) };
    let options = CFDictionary::from_CFType_pairs(&[(key, CFBoolean::true_value())]);
    // SAFETY: `options` is a valid CFDictionary that lives for this call.
    unsafe { AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef()) }
}

pub fn is_trusted() -> bool {
    // SAFETY: AXIsProcessTrusted does not prompt and has no pointer arguments.
    unsafe { AXIsProcessTrusted() }
}

pub fn focused_window_title(pid: i32) -> Option<String> {
    let app = application_element(pid)?;
    let window = ax_copy(&app, "AXFocusedWindow")?;
    ax_string(&window, "AXTitle")
}

pub fn selected_text(pid: i32) -> Option<String> {
    let app = application_element(pid)?;
    if let Some(focused) = ax_copy(&app, "AXFocusedUIElement")
        && let Some(text) = ax_string(&focused, "AXSelectedText")
    {
        return Some(text);
    }
    let system = system_wide()?;
    let focused = ax_copy(&system, "AXFocusedUIElement")?;
    ax_string(&focused, "AXSelectedText")
}

pub(crate) fn application_element(pid: i32) -> Option<CFType> {
    // SAFETY: AXUIElementCreateApplication returns a +1 CF object or null.
    let raw = unsafe { AXUIElementCreateApplication(pid) };
    wrap_create(raw)
}

pub(crate) fn snapshot_root(pid: i32) -> Option<CFType> {
    let app = application_element(pid)?;
    if let Some(window) = ax_copy(&app, "AXFocusedWindow") {
        return Some(window);
    }
    ax_array(&app, "AXWindows").into_iter().next().or(Some(app))
}

pub(crate) fn ax_copy(element: &CFType, attribute: &str) -> Option<CFType> {
    let name = CFString::new(attribute);
    let mut value: CFTypeRef = ptr::null();
    // SAFETY: `element` is a live AXUIElement; CopyAttributeValue writes a +1 value.
    let err = unsafe {
        AXUIElementCopyAttributeValue(
            element.as_CFTypeRef() as AxUiElementRef,
            name.as_concrete_TypeRef(),
            &mut value,
        )
    };
    if err != AX_SUCCESS || value.is_null() {
        return None;
    }
    Some(unsafe { CFType::wrap_under_create_rule(value) })
}

pub(crate) fn ax_string(element: &CFType, attribute: &str) -> Option<String> {
    ax_string_raw(element, attribute).filter(|text| !text.is_empty())
}

pub(crate) fn ax_string_raw(element: &CFType, attribute: &str) -> Option<String> {
    let value = ax_copy(element, attribute)?;
    value.downcast::<CFString>().map(|text| text.to_string())
}

pub(crate) fn ax_bool(element: &CFType, attribute: &str) -> Option<bool> {
    let value = ax_copy(element, attribute)?;
    value.downcast::<CFBoolean>().map(bool::from)
}

pub(crate) fn ax_children(element: &CFType) -> Vec<CFType> {
    ax_array(element, "AXChildren")
}

pub(crate) fn ax_array(element: &CFType, attribute: &str) -> Vec<CFType> {
    let Some(value) = ax_copy(element, attribute) else {
        return Vec::new();
    };
    let Some(array) = value.downcast::<CFArray>() else {
        return Vec::new();
    };
    array
        .get_all_values()
        .into_iter()
        .filter_map(|ptr| {
            if ptr.is_null() {
                None
            } else {
                // SAFETY: AX array values are CFTypes retained by the array.
                Some(unsafe { CFType::wrap_under_get_rule(ptr as CFTypeRef) })
            }
        })
        .collect()
}

pub(crate) fn ax_action_names(element: &CFType) -> Vec<String> {
    let mut names: CfArrayRef = ptr::null();
    // SAFETY: CopyActionNames writes a +1 CFArray of CFString, or fails.
    let err =
        unsafe { AXUIElementCopyActionNames(element.as_CFTypeRef() as AxUiElementRef, &mut names) };
    if err != AX_SUCCESS || names.is_null() {
        return Vec::new();
    }
    let array: CFArray = unsafe { CFArray::wrap_under_create_rule(names as _) };
    array
        .get_all_values()
        .into_iter()
        .filter_map(|ptr| {
            if ptr.is_null() {
                return None;
            }
            let value = unsafe { CFType::wrap_under_get_rule(ptr as CFTypeRef) };
            value
                .downcast::<CFString>()
                .map(|text| text.to_string())
                .filter(|text| !text.is_empty())
        })
        .collect()
}

pub(crate) fn ax_press(element: &CFType) -> Result<(), String> {
    let actions = ax_action_names(element);
    if !actions.is_empty() && !actions.iter().any(|name| name == "AXPress") {
        return Err("this control does not support press".into());
    }
    let action = CFString::new("AXPress");
    // SAFETY: `element` is a live AXUIElement; `action` lives for the call.
    let err = unsafe {
        AXUIElementPerformAction(
            element.as_CFTypeRef() as AxUiElementRef,
            action.as_concrete_TypeRef(),
        )
    };
    if err == AX_SUCCESS {
        Ok(())
    } else {
        Err("press failed".into())
    }
}

pub(crate) fn ax_set_value(element: &CFType, value: &str) -> Result<(), String> {
    let attribute = CFString::new("AXValue");
    let cf_value = CFString::new(value);
    // SAFETY: `element` is a live AXUIElement; attribute and value live for the call.
    let err = unsafe {
        AXUIElementSetAttributeValue(
            element.as_CFTypeRef() as AxUiElementRef,
            attribute.as_concrete_TypeRef(),
            cf_value.as_CFTypeRef(),
        )
    };
    if err == AX_SUCCESS {
        Ok(())
    } else {
        Err("set value failed".into())
    }
}

fn system_wide() -> Option<CFType> {
    // SAFETY: AXUIElementCreateSystemWide returns a +1 CF object or null.
    let raw = unsafe { AXUIElementCreateSystemWide() };
    wrap_create(raw)
}

fn wrap_create(raw: AxUiElementRef) -> Option<CFType> {
    if raw.is_null() {
        None
    } else {
        // SAFETY: caller used a Create rule; `raw` is a CFType.
        Some(unsafe { CFType::wrap_under_create_rule(raw as CFTypeRef) })
    }
}
