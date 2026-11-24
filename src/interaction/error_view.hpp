#pragma once

#include <string>

#include <ftxui/dom/elements.hpp>

namespace booth {

/// A capped red error strip with a retry hint.
ftxui::Element error_banner_view(const std::string& error);

}  // namespace booth
