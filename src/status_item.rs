#![allow(unexpected_cfgs)]

use crate::ui::UiCommand;
use anyhow::{Context, Result, bail};
use cocoa::appkit::{
    NSButton, NSColor, NSImage, NSRectFill, NSSquareStatusItemLength, NSStatusBar, NSStatusItem,
};
use cocoa::base::{NO, YES, id, nil};
use cocoa::foundation::{NSPoint, NSRect, NSSize, NSString};
use objc::declare::ClassDecl;
use objc::rc::StrongPtr;
use objc::runtime::{Class, Object, Sel};
use objc::{class, msg_send, sel, sel_impl};
use std::ffi::c_void;
use std::sync::OnceLock;
use std::sync::mpsc::Sender;

const CALLBACK_IVAR: &str = "callback";

type ClickHandler = Box<dyn FnMut()>;

pub struct StatusItemController {
    item: StrongPtr,
    _target: StrongPtr,
}

impl StatusItemController {
    pub fn install(ui_tx: Sender<UiCommand>) -> Result<Self> {
        unsafe {
            let button_width = NSSquareStatusItemLength;
            let status_bar = NSStatusBar::systemStatusBar(nil);
            let item = StrongPtr::retain(status_bar.statusItemWithLength_(button_width));
            let button = item.button();

            if button == nil {
                bail!("macOS status item did not provide a button");
            }

            item.setLength_(button_width);
            let _: () = msg_send![button, setHidden: NO];
            button.setTitle_(NSString::alloc(nil).init_str(""));
            let image = status_item_image();
            if image != nil {
                button.setImage_(image);
            } else {
                button.setTitle_(NSString::alloc(nil).init_str("TV"));
            }
            let _: () = msg_send![button, setFrameSize: NSSize::new(button_width, 22.0)];
            let _: () = msg_send![button, setToolTip: NSString::alloc(nil).init_str("Open tvoice settings")];

            let target: id = msg_send![status_item_target_class(), new];
            if target == nil {
                bail!("failed to create status item target");
            }

            let handler: ClickHandler = Box::new(move || {
                let _ = ui_tx.send(UiCommand::ShowSettings);
            });

            (*target).set_ivar(
                CALLBACK_IVAR,
                Box::into_raw(Box::new(handler)) as *mut c_void,
            );

            button.setTarget_(target);
            button.setAction_(sel!(clicked:));

            Ok(Self {
                item,
                _target: StrongPtr::new(target),
            })
        }
    }
}

impl Drop for StatusItemController {
    fn drop(&mut self) {
        unsafe {
            let status_bar = NSStatusBar::systemStatusBar(nil);
            status_bar.removeStatusItem_(*self.item);
        }
    }
}

fn status_item_target_class() -> &'static Class {
    static CLASS: OnceLock<&'static Class> = OnceLock::new();

    CLASS.get_or_init(|| unsafe {
        let mut decl = ClassDecl::new("TvoiceStatusItemTarget", class!(NSObject))
            .context("failed to declare TvoiceStatusItemTarget")
            .unwrap();
        decl.add_ivar::<*mut c_void>(CALLBACK_IVAR);
        decl.add_method(
            sel!(clicked:),
            status_item_clicked as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(sel!(dealloc), dealloc_target as extern "C" fn(&Object, Sel));
        decl.register()
    })
}

fn status_item_image() -> id {
    unsafe {
        let size = NSSize::new(18.0, 18.0);
        let image = NSImage::alloc(nil).initWithSize_(size);
        if image == nil {
            return nil;
        }

        image.lockFocus();

        let fill = NSColor::colorWithCalibratedRed_green_blue_alpha_(nil, 0.0, 0.0, 0.0, 1.0);
        let _: () = msg_send![fill, set];

        NSRectFill(NSRect::new(NSPoint::new(3.0, 4.0), NSSize::new(3.0, 7.0)));
        NSRectFill(NSRect::new(NSPoint::new(7.5, 3.0), NSSize::new(3.0, 11.0)));
        NSRectFill(NSRect::new(NSPoint::new(12.0, 5.0), NSSize::new(3.0, 6.0)));

        image.unlockFocus();
        let _: () = msg_send![image, setTemplate: YES];
        let _: () = msg_send![image, setSize: size];

        image
    }
}

extern "C" fn status_item_clicked(this: &Object, _: Sel, _: id) {
    unsafe {
        let raw_handler = *this.get_ivar::<*mut c_void>(CALLBACK_IVAR);
        if raw_handler.is_null() {
            return;
        }

        let handler = &mut *(raw_handler as *mut ClickHandler);
        handler();
    }
}

extern "C" fn dealloc_target(this: &Object, _: Sel) {
    unsafe {
        let raw_handler = *this.get_ivar::<*mut c_void>(CALLBACK_IVAR);
        if !raw_handler.is_null() {
            drop(Box::from_raw(raw_handler as *mut ClickHandler));
        }

        let _: () = msg_send![super(this, class!(NSObject)), dealloc];
    }
}
