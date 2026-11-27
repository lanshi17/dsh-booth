#include "interaction/provider_edit.hpp"

#include <algorithm>
#include <cctype>
#include <chrono>
#include <sstream>
#include <string>
#include <string_view>

#include "interaction/external_editor.hpp"

namespace booth {
namespace {

std::string trim(std::string_view text) {
  while (!text.empty() && std::isspace(static_cast<unsigned char>(text.front())) != 0) {
    text.remove_prefix(1);
  }
  while (!text.empty() && std::isspace(static_cast<unsigned char>(text.back())) != 0) {
    text.remove_suffix(1);
  }
  return std::string(text);
}

std::string value_after(std::string_view line, std::string_view key) {
  if (line.rfind(key, 0) != 0) return {};
  return trim(line.substr(key.size()));
}

}  // namespace

void edit_provider(ControlClient& control, ftxui::ScreenInteractive& screen, AppState& state,
                   std::string provider) {
  provider = trim(provider);
  if (provider.empty()) {
    state.notice = "usage: /providers edit <provider>";
    return;
  }
  auto current = control.request("providers.get", {{"provider", provider}}, std::chrono::seconds(10));
  if (!current) {
    state.notice = current.error().detail;
    return;
  }
  const std::string base_url = current->value("baseURL", std::string{});
  const bool key_configured = current->value("apiKeyConfigured", false);
  const std::string masked = current->value("apiKeyMasked", std::string{});

  std::ostringstream seed;
  seed << "# provider: " << provider << "\n"
       << "# baseURL: empty restores the provider default\n"
       << "# apiKey: leave blank to keep the current key\n";
  if (key_configured) seed << "# current key: configured " << masked << "\n";
  else seed << "# current key: not configured\n";
  seed << "\nbaseURL=" << base_url << "\napiKey=\n";
  const std::string original = seed.str();
  std::string text = original;
  if (!edit_in_screen(screen, text, state.notice)) return;
  if (text == original) {
    state.notice = "provider unchanged";
    return;
  }

  std::string next_base = base_url;
  std::string api_key;
  std::istringstream input(text);
  std::string line;
  while (std::getline(input, line)) {
    const std::string_view view(line);
    if (view.rfind("baseURL=", 0) == 0) next_base = value_after(view, "baseURL=");
    else if (view.rfind("apiKey=", 0) == 0) api_key = value_after(view, "apiKey=");
  }
  nlohmann::json params = {{"provider", provider}, {"baseURL", next_base}};
  if (!api_key.empty()) params["apiKey"] = api_key;
  auto saved = control.request("providers.update", params, std::chrono::seconds(60));
  state.notice = saved ? saved->value("text", std::string{"provider updated"}) : saved.error().detail;
}

}  // namespace booth
