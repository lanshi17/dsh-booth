#include "interaction/tui_internal.hpp"

#include <utility>

#include <ftxui/component/event.hpp>

namespace booth {

void post_event(Inbox& inbox, ftxui::ScreenInteractive& screen, Event event) {
  {
    std::lock_guard<std::mutex> lock(inbox.mutex);
    inbox.events.push_back(std::move(event));
  }
  screen.PostEvent(ftxui::Event::Custom);
}

}  // namespace booth
