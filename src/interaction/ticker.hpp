#pragma once

#include <atomic>
#include <thread>

#include <ftxui/component/screen_interactive.hpp>

namespace booth {

/// Posts a redraw every 120 ms while `animate` is set; stops when `alive` clears.
std::thread start_ticker(ftxui::ScreenInteractive& screen, std::atomic<bool>& animate,
                         std::atomic<bool>& alive);

}  // namespace booth
