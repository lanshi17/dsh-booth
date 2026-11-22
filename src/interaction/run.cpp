#include "interaction/tui.hpp"
#include "interaction/tui_internal.hpp"

#include <ftxui/component/screen_interactive.hpp>

#include <cstdlib>
#include <iostream>
#include <mutex>
#include <string>

#include "interaction/state.hpp"
#include "interaction/view.hpp"
#include "subagent/kernel.hpp"
#include "supervisor/daemon.hpp"
#include "supervisor/supervisor.hpp"

namespace booth {
namespace {

using ftxui::ScreenInteractive;

}  // namespace

int run_tui(const Config& config, const Env& env) {
  // omp always renders its palette; NO_COLOR/TERM=dumb in the launching shell would otherwise
  // flatten booth to the terminal default and lose the status band.
  ::unsetenv("NO_COLOR");
  const char* term = ::getenv("TERM");
  if (term == nullptr || std::string(term) == "dumb") ::setenv("TERM", "xterm-256color", 1);
  if (::getenv("COLORTERM") == nullptr) ::setenv("COLORTERM", "truecolor", 1);
  auto screen = ScreenInteractive::TerminalOutput();
  screen.TrackMouse(true);
  screen.ForceHandleCtrlC(false);
  Inbox inbox;
  AppState state;
  const auto sink = [&](Event event) {
    {
      std::lock_guard<std::mutex> lock(inbox.mutex);
      inbox.events.push_back(std::move(event));
    }
    screen.PostEvent(ftxui::Event::Custom);
  };
  if (const int fd = connect_daemon(config.socket_file); fd >= 0) {
    RemoteKernel kernel(fd, sink);
    booth::print_splash();
    return tui_loop(screen, kernel, state, inbox, config);
  }
  Supervisor supervisor(config, env, sink, BOOTH_VERSION);
  auto up = supervisor.up();
  if (!up) {
    std::cerr << "booth: " << to_string(up.error()) << '\n';
    return 1;
  }
  state.snapshot.phase = DshPhase::Ready;
  booth::print_splash();
  const int code = tui_loop(screen, supervisor, state, inbox, config);
  auto down = supervisor.down();
  if (!down) {
    std::cerr << "booth: " << to_string(down.error()) << '\n'; return 1;
  }
  return code;
}
}  // namespace booth
