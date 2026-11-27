#pragma once

#include <optional>
#include <string>

#include "contract/event.hpp"

namespace booth {

/// Persist an "allow always" grant for the exact tool request shape.
bool remember_approval(const std::string& path, const PermissionRequest& request, std::string& error);

/// Return the current request's allow-always option id when a stored grant matches.
std::optional<std::string> remembered_approval(const std::string& path, const PermissionRequest& request);

/// Forget every remembered grant.
bool clear_approvals(const std::string& path, std::string& error);

}  // namespace booth
