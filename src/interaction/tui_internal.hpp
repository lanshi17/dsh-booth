#pragma once

#include <cstddef>
#include <deque>
#include <mutex>
#include <set>

#include <ftxui/component/screen_interactive.hpp>

#include "contract/event.hpp"
#include "interaction/paste_buffer.hpp"
#include "supervisor/config.hpp"

namespace booth {

struct AppState;
class Kernel;

/// Events posted by kernel reader threads, drained on the UI thread.
struct Inbox {
  std::mutex mutex;
  std::deque<Event> events;
};

int tui_loop(ftxui::ScreenInteractive& screen, Kernel& kernel, AppState& state, Inbox& inbox,
             const Config& config);

/// Toggles a transcript entry's disclosure; live thoughts and non-tool entries are ignored.
void toggle_entry(AppState& state, std::set<std::size_t>& open_tools, std::size_t index);

}  // namespace booth
