#include <CLI/CLI.hpp>

#include <unistd.h>

#include <chrono>
#include <iostream>
#include <memory>
#include <string>
#include <thread>

#include "contract/event.hpp"
#include "control/client.hpp"
#include "interaction/theme.hpp"
#include "provider/editor.hpp"
#include "provider/model.hpp"
#include "supervisor/config.hpp"
#include "supervisor/supervisor.hpp"

namespace {

using booth::ControlClient;
using booth::Supervisor;
using booth::provider::Listing;

std::string failure(const booth::Error& error) { return booth::to_string(error); }

/// Retry the provider directory while a fresh dsh profile boots.
Listing wait_for_listing(ControlClient& control) {
  Listing listing = booth::provider::list(control);
  for (int attempt = 0; attempt < 60 && !listing.error.empty(); ++attempt) {
    std::this_thread::sleep_for(std::chrono::milliseconds(500));
    listing = booth::provider::list(control);
  }
  return listing;
}

}  // namespace

int main(int argc, char** argv) {
  CLI::App app{"Edit dsh provider routes in a standalone TUI"};
  app.name("booth-provider");
  app.set_version_flag("-V,--version", BOOTH_VERSION);
  std::string provider;
  app.add_option("-p,--provider", provider, "Open this route directly");
  bool create = false;
  app.add_flag("-n,--new", create, "Start a new route");
  try {
    app.parse(argc, argv);
  } catch (const CLI::ParseError& error) {
    return app.exit(error);
  }
  if (::isatty(STDIN_FILENO) == 0) {
    std::cerr << "booth-provider: the editor needs a terminal\n";
    return 1;
  }

  auto config = booth::Config::from_env(booth::system_env());
  if (!config) {
    std::cerr << "booth-provider: " << failure(config.error()) << '\n';
    return 1;
  }
  const booth::Env env = booth::system_env();
  booth::apply_theme(booth::load_theme(config->state_dir / "theme.conf"));

  ControlClient control;
  control.set_path(config->control_socket);
  Listing listing = booth::provider::list(control);
  std::unique_ptr<Supervisor> temporal;
  if (!listing.error.empty()) {
    // No live supervisor: start dsh long enough to edit its settings.
    auto supervisor = std::make_unique<Supervisor>(*config, env, [](booth::Event) {}, BOOTH_VERSION);
    auto up = supervisor->up();
    if (up) {
      temporal = std::move(supervisor);
      std::cerr << "booth-provider: starting dsh for provider settings\n";
    }
    listing = wait_for_listing(control);
  }
  if (!listing.error.empty()) {
    std::cerr << "booth-provider: " << listing.error << '\n';
    if (temporal) temporal->down();
    return 1;
  }

  const int code = booth::provider::run_editor(control, std::move(listing), provider, create);
  if (temporal) temporal->down();
  return code;
}
