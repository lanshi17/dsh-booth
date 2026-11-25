#pragma once

#include <string>
#include <vector>

#include <ftxui/dom/elements.hpp>

namespace booth {

std::string normalize_lang(std::string lang);

std::string expand_code_tabs(const std::string& line);

/// Render one code line as visual rows no wider than the terminal, keeping
/// the highlighter's spans. A long line wraps instead of being clipped or
/// having characters dropped by FTXUI's hbox shrink.
std::vector<ftxui::Elements> code_rows(const std::string& line, const std::string& lang);

}  // namespace booth
