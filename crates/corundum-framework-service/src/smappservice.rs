//! SMAppService, through objc2: the registration of an agent a macOS package carries.
//!
//! The one place in the crate that calls into ServiceManagement. The flows reach it through
//! [`ServiceManager::app_service`](crate::ServiceManager::app_service), so no test does.

use objc2_foundation::NSString;
use objc2_service_management::{SMAppService, SMAppServiceStatus};

use crate::error::{Error, Result};
use crate::manager::{AppServiceAction, AppServiceStatus};

/// Carry out `action` on the agent whose plist the running `.app` carries as
/// `Contents/Library/LaunchAgents/<plist_name>`, and return its status afterwards.
pub(crate) fn app_service(plist_name: &str, action: AppServiceAction) -> Result<AppServiceStatus> {
    // SAFETY: `agentServiceWithPlistName:` takes any string and returns a service object;
    // a name the bundle does not carry is reported by the status as `NotFound`.
    let service =
        unsafe { SMAppService::agentServiceWithPlistName(&NSString::from_str(plist_name)) };
    // SAFETY: both are plain synchronous calls on the service object, which is alive for the
    // whole call; any refusal comes back as the `NSError`.
    let outcome = match action {
        AppServiceAction::Register => unsafe { service.registerAndReturnError() },
        AppServiceAction::Unregister => unsafe { service.unregisterAndReturnError() },
        AppServiceAction::Status => Ok(()),
    };
    outcome.map_err(|error| Error::Manager(error.localizedDescription().to_string()))?;
    // SAFETY: a read of the service object's status, which has no preconditions.
    let status = unsafe { service.status() };
    Ok(match status {
        SMAppServiceStatus::NotRegistered => AppServiceStatus::NotRegistered,
        SMAppServiceStatus::Enabled => AppServiceStatus::Enabled,
        SMAppServiceStatus::RequiresApproval => AppServiceStatus::RequiresApproval,
        // `NotFound`, and any status a later macOS adds: either way there is no agent this
        // crate can use.
        _ => AppServiceStatus::NotFound,
    })
}
