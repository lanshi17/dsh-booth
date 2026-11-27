#pragma once

#include <string>

#include <ftxui/component/screen_interactive.hpp>

#include "control/client.hpp"
#include "interaction/state.hpp"

namespace booth {

/// Handles `/providers add|edit|remove|help`. Returns false for bare `/providers`
/// and `/providers list`, which the library overlay owns.
bool handle_provider_command(ControlClient& control, ftxui::ScreenInteractive& screen,
                             AppState& state, const std::string& line);

/// Edit one provider's display name, protocol, models, baseURL and API key.
/// `create` refuses an existing user profile; otherwise an unconfigured route is
/// refused so add/edit cannot silently swap behavior.
void edit_provider(ControlClient& control, ftxui::ScreenInteractive& screen, AppState& state,
                   std::string provider, bool create = false);

/// Remove one user-level provider profile; `remove_key` also deletes its key.
void remove_provider(ControlClient& control, ftxui::ScreenInteractive& screen, AppState& state,
                     std::string provider, bool remove_key);

}  // namespace booth
