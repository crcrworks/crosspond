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
    fn AXValueGetValue(value: CFTypeRef, value_type: u32, value_ptr: *mut std::ffi::c_void) -> u8;
}

const AX_VALUE_CG_POINT: u32 = 1;

#[repr(C)]
struct CgPoint {
    x: f64,
    y: f64,
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

/// Ask the target app to expose a fuller AX tree and accept actions while
/// it stays in the background (same flags Codex Computer Use sets).
pub(crate) fn enable_background_ax(pid: i32) {
    let Some(app) = application_element(pid) else {
        return;
    };
    let _ = ax_set_bool(&app, "AXEnhancedUserInterface", true);
    let _ = ax_set_bool(&app, "AXManualAccessibility", true);
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
    let role = ax_string(element, "AXRole").unwrap_or_default();
    let subrole = ax_string(element, "AXSubrole").unwrap_or_default();
    if is_unsafe_press_target(&role, &subrole) {
        return Err(
            "won't press the window or its close/minimize controls; pick a specific button or field"
                .into(),
        );
    }
    if is_text_role(&role) {
        return ax_focus(element);
    }
    if origin_hits_traffic_lights(element) {
        return Err(
            "won't press this control; it sits on the window's close/minimize buttons".into(),
        );
    }
    let actions = ax_action_names(element);
    if actions.iter().any(|name| name == "AXPress") {
        return ax_perform(element, "AXPress");
    }
    if actions.iter().any(|name| name == "AXConfirm") {
        return ax_perform(element, "AXConfirm");
    }
    Err("this control does not support press".into())
}

pub(crate) fn ax_focus(element: &CFType) -> Result<(), String> {
    ax_set_bool(element, "AXFocused", true).map_err(|_| "could not focus this control".into())
}

fn ax_set_bool(element: &CFType, attribute: &str, value: bool) -> Result<(), String> {
    let attribute = CFString::new(attribute);
    let value = if value {
        CFBoolean::true_value()
    } else {
        CFBoolean::false_value()
    };
    // SAFETY: `element` is a live AXUIElement; attribute and value live for the call.
    let err = unsafe {
        AXUIElementSetAttributeValue(
            element.as_CFTypeRef() as AxUiElementRef,
            attribute.as_concrete_TypeRef(),
            value.as_CFTypeRef(),
        )
    };
    if err == AX_SUCCESS {
        Ok(())
    } else {
        Err("set attribute failed".into())
    }
}

pub(crate) fn ax_set_value(element: &CFType, value: &str) -> Result<(), String> {
    let _ = ax_focus(element);
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

fn ax_perform(element: &CFType, action: &str) -> Result<(), String> {
    let action = CFString::new(action);
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

fn is_unsafe_press_target(role: &str, subrole: &str) -> bool {
    matches!(
        role,
        "AXWindow"
            | "AXApplication"
            | "AXCloseButton"
            | "AXMinimizeButton"
            | "AXZoomButton"
            | "AXFullScreenButton"
            | "AXGrowArea"
    ) || is_chrome_subrole(subrole)
}

fn is_text_role(role: &str) -> bool {
    matches!(
        role,
        "AXTextField" | "AXTextArea" | "AXComboBox" | "AXSearchField" | "AXSecureTextField"
    )
}

pub(crate) fn is_chrome_subrole(subrole: &str) -> bool {
    matches!(
        subrole,
        "AXCloseButton" | "AXMinimizeButton" | "AXZoomButton" | "AXFullScreenButton"
    )
}

/// Chromium often maps AXPress to a click at the element's origin. For a
/// window-sized node that origin is the traffic lights.
pub(crate) fn origin_in_traffic_lights(window: (f64, f64), element: (f64, f64)) -> bool {
    let dx = element.0 - window.0;
    let dy = element.1 - window.1;
    (-2.0..80.0).contains(&dx) && (-2.0..40.0).contains(&dy)
}

fn origin_hits_traffic_lights(element: &CFType) -> bool {
    let window = ax_copy(element, "AXWindow").or_else(|| ax_copy(element, "AXTopLevelUIElement"));
    let Some(window) = window else {
        return false;
    };
    let Some(window_origin) = ax_position(&window) else {
        return false;
    };
    let Some(element_origin) = ax_position(element) else {
        return false;
    };
    origin_in_traffic_lights(window_origin, element_origin)
}

fn ax_position(element: &CFType) -> Option<(f64, f64)> {
    let value = ax_copy(element, "AXPosition")?;
    let mut point = CgPoint { x: 0.0, y: 0.0 };
    // SAFETY: AXPosition is an AXValue CGPoint; CGFloat is f64 on 64-bit macOS.
    let ok = unsafe {
        AXValueGetValue(
            value.as_CFTypeRef(),
            AX_VALUE_CG_POINT,
            std::ptr::addr_of_mut!(point).cast(),
        )
    };
    if ok == 0 {
        None
    } else {
        Some((point.x, point.y))
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

#[cfg(test)]
mod tests {
    use super::origin_in_traffic_lights;

    #[test]
    fn window_origin_is_traffic_lights() {
        assert!(origin_in_traffic_lights((100.0, 80.0), (100.0, 80.0)));
        assert!(origin_in_traffic_lights((100.0, 80.0), (120.0, 90.0)));
        assert!(!origin_in_traffic_lights((100.0, 80.0), (220.0, 80.0)));
        assert!(!origin_in_traffic_lights((100.0, 80.0), (100.0, 160.0)));
    }
}
