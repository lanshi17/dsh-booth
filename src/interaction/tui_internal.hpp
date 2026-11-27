#pragma once

#include <cstddef>
#include <deque>
#include <mutex>
#include <set>

#include <ftxui/component/screen_interactive.hpp>

#include "contract/event.hpp"
#include "interaction/library_ui.hpp"
#include "interaction/paste_buffer.hpp"
#include "supervisor/config.hpp"

namespace booth {

struct AppState;
class Kernel;
class ControlClient;

/// Events posted by kernel reader threads, drained on the UI thread.
struct Inbox {
  std::mutex mutex;
  std::deque<Event> events;
};

/// Push one event and wake the UI loop; safe from kernel and worker threads.
void post_event(Inbox& inbox, ftxui::ScreenInteractive& screen, Event event);

int tui_loop(ftxui::ScreenInteractive& screen, Kernel& kernel, AppState& state, Inbox& inbox,
             const Config& config, ControlClient& control);

/// Toggles a transcript entry's disclosure; live thoughts and non-tool entries are ignored.
void toggle_entry(AppState& state, std::set<std::size_t>& open_tools, std::size_t index);

}  // namespace booth
