#pragma once

#include <cstddef>
#include <functional>
#include <string>

#include "contract/options.hpp"
#include "interaction/copy_picker.hpp"
#include "interaction/editor.hpp"
#include "interaction/file_menu.hpp"
#include "interaction/history_search.hpp"
#include "interaction/mention_menu.hpp"
#include "interaction/model_picker.hpp"
#include "interaction/mouse_zone.hpp"
#include "interaction/state.hpp"
#include "interaction/view.hpp"

namespace booth {

struct MouseActions {
  AppState& state;
  Editor& editor;
  SlashMenu& slash;
  FileMenu& files;
  ModelPicker& model;
  SessionPicker& sessions;
  HistorySearch& history;
  CopyPicker& copy;
  MentionMenu& mention;
  std::function<void()> submit;
  std::function<void(const ModelChoice&)> apply_model;
  std::function<void(const SessionRow&)> resume;
  std::function<void(std::size_t line, std::size_t column)> cursor_to;
  std::function<void(std::size_t entry)> toggle_tool;
  std::function<void(const char*)> permission;
  std::function<void()> close_help;
};

/// Executes the action behind one rendered mouse hit. Returns false only when the
/// hit has no action (the caller may still consume the click).
bool click_mouse(const MouseHit& hit, MouseActions& actions);

}  // namespace booth
