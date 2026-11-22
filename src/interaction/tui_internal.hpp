#pragma once

#include <deque>
#include <mutex>

#include <ftxui/component/screen_interactive.hpp>

#include "contract/event.hpp"
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

}  // namespace booth
