#pragma once

#include <cstddef>
#include <string>

#include <ftxui/dom/elements.hpp>

#include "interaction/tool_parse.hpp"

namespace booth {

std::string skill_title(const SkillResult& skill);
ftxui::Elements skill_rows(const SkillResult& skill, bool expanded, std::size_t content_width);

}  // namespace booth
