#pragma once

#include <string>

#include <ftxui/component/screen_interactive.hpp>

#include "control/client.hpp"
#include "interaction/state.hpp"

namespace booth {

/// Edit one provider's baseURL and API key through an external editor.
/// The API key field is blank to keep the current key; the value is never
/// printed into the transcript or persisted in booth's draft/history files.
void edit_provider(ControlClient& control, ftxui::ScreenInteractive& screen, AppState& state,
                   std::string provider);

}  // namespace booth
