#include "interaction/ticker.hpp"

#include <chrono>

namespace booth {

std::thread start_ticker(ftxui::ScreenInteractive& screen, std::atomic<bool>& animate,
                         std::atomic<bool>& alive) {
  return std::thread([&] {
    while (alive) {
      std::this_thread::sleep_for(std::chrono::milliseconds(120));
      if (animate) screen.PostEvent(ftxui::Event::Custom);
    }
  });
}

}  // namespace booth
