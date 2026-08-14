//! Accessibility attribute reads for ambient context.
//!
//! `unsafe` stays here: these are C APIs from the ApplicationServices framework.
//! Phase 3 only reads focused window title and selected text. It does not walk
//! the full AX tree or perform UI actions.

use std::ptr;

use core_foundation::base::{CFType, CFTypeRef, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::CFDictionary;
use core_foundation::string::{CFString, CFStringRef};

const AX_SUCCESS: i32 = 0;

type AxUiElementRef = *const std::ffi::c_void;

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    static kAXTrustedCheckOptionPrompt: CFStringRef;
    fn AXIsProcessTrustedWithOptions(options: core_foundation::dictionary::CFDictionaryRef)
    -> bool;
    fn AXUIElementCreateSystemWide() -> AxUiElementRef;
    fn AXUIElementCreateApplication(pid: i32) -> AxUiElementRef;
    fn AXUIElementCopyAttributeValue(
        element: AxUiElementRef,
        attribute: CFStringRef,
        value: *mut CFTypeRef,
    ) -> i32;
}

pub fn prompt_and_is_trusted() -> bool {
    let key = unsafe { CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt) };
    let options = CFDictionary::from_CFType_pairs(&[(key, CFBoolean::true_value())]);
    // SAFETY: `options` is a valid CFDictionary that lives for this call.
    unsafe { AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef()) }
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

fn system_wide() -> Option<CFType> {
    // SAFETY: AXUIElementCreateSystemWide returns a +1 CF object or null.
    let raw = unsafe { AXUIElementCreateSystemWide() };
    wrap_create(raw)
}

fn application_element(pid: i32) -> Option<CFType> {
    // SAFETY: AXUIElementCreateApplication returns a +1 CF object or null.
    let raw = unsafe { AXUIElementCreateApplication(pid) };
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

fn ax_copy(element: &CFType, attribute: &str) -> Option<CFType> {
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

fn ax_string(element: &CFType, attribute: &str) -> Option<String> {
    let value = ax_copy(element, attribute)?;
    value
        .downcast::<CFString>()
        .map(|text| text.to_string())
        .filter(|text| !text.is_empty())
}
