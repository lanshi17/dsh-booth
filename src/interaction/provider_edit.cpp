#include "interaction/provider_edit.hpp"

#include <algorithm>
#include <cctype>
#include <chrono>
#include <sstream>
#include <string>
#include <string_view>
#include <vector>

#include <nlohmann/json.hpp>

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

std::string join_models(const nlohmann::json& models) {
  std::string out;
  if (!models.is_array()) return out;
  for (const auto& model : models) {
    const std::string id = model.is_object() ? model.value("id", std::string{}) : model.get<std::string>();
    if (id.empty()) continue;
    if (!out.empty()) out += ", ";
    out += id;
  }
  return out;
}

std::vector<std::string> split_models(std::string_view text) {
  std::vector<std::string> out;
  std::string current;
  for (const char ch : text) {
    if (ch == ',' || ch == '\n' || ch == '\r') {
      const std::string id = trim(current);
      if (!id.empty()) out.push_back(id);
      current.clear();
    } else {
      current += ch;
    }
  }
  const std::string id = trim(current);
  if (!id.empty()) out.push_back(id);
  return out;
}

void force_repaint(ftxui::ScreenInteractive& screen) {
  screen.RequestAnimationFrame();
  screen.PostEvent(ftxui::Event::Custom);
}

}  // namespace

bool handle_provider_command(ControlClient& control, ftxui::ScreenInteractive& screen,
                             AppState& state, const std::string& line) {
  const std::string_view text(line);
  const std::string_view prefix = "/providers";
  if (text.rfind(prefix, 0) != 0) return false;
  if (text.size() > prefix.size() && !std::isspace(static_cast<unsigned char>(text[prefix.size()]))) {
    return false;
  }
  const std::string rest = trim(text.size() > prefix.size() ? text.substr(prefix.size()) : std::string_view{});
  if (rest.empty() || rest == "list") return false;
  const auto split = rest.find_first_of(" \t");
  const std::string verb = rest.substr(0, split);
  const std::string argument = split == std::string::npos ? std::string{} : trim(rest.substr(split + 1));
  if (verb == "edit" || verb == "add") {
    if (argument.empty() || argument.find_first_of(" \t") != std::string::npos) {
      state.notice = "usage: /providers " + verb + " <provider>";
      return true;
    }
    edit_provider(control, screen, state, argument, verb == "add");
    return true;
  }
  if (verb == "remove") {
    if (argument.empty()) {
      state.notice = "usage: /providers remove <provider> [--key]";
      return true;
    }
    const auto space = argument.find_first_of(" \t");
    const std::string provider = argument.substr(0, space);
    const std::string option = space == std::string::npos ? std::string{} : trim(argument.substr(space + 1));
    if (!option.empty() && option != "--key" && option != "--with-key") {
      state.notice = "usage: /providers remove <provider> [--key]";
      return true;
    }
    remove_provider(control, screen, state, provider, !option.empty());
    return true;
  }
  if (verb == "models" || verb == "discover") {
    if (argument.empty() || argument.find_first_of(" \t") != std::string::npos) {
      state.notice = "usage: /providers models <provider>";
      return true;
    }
    discover_provider_models(control, screen, state, argument);
    return true;
  }
  if (verb == "help") {
    state.notice =
        "usage: /providers [list] · /providers add|edit <provider> · /providers models <provider> · "
        "/providers remove <provider> [--key]";
    return true;
  }
  state.notice = "usage: /providers [list|add|edit|remove] <provider>";
  return true;
}

void edit_provider(ControlClient& control, ftxui::ScreenInteractive& screen, AppState& state,
                   std::string provider, bool create) {
  provider = trim(provider);
  if (provider.empty()) {
    state.notice = create ? "usage: /providers add <provider>" : "usage: /providers edit <provider>";
    return;
  }
  auto current = control.request("providers.get", {{"provider", provider}}, std::chrono::seconds(10));
  if (!current) {
    state.notice = current.error().detail;
    return;
  }
  const bool configured = current->value("configured", true);
  if (create && configured) {
    state.notice = provider + " is already configured; use /providers edit " + provider;
    return;
  }
  const std::string display_name = current->value("displayName", provider);
  const std::string base_url = current->value("baseURL", std::string{});
  const std::string api = current->value("api", std::string{});
  const std::string models = join_models(current->value("models", nlohmann::json::array()));
  const bool key_configured = current->value("apiKeyConfigured", false);
  const std::string masked = current->value("apiKeyMasked", std::string{});

  std::ostringstream seed;
  seed << "# provider: " << provider << (configured ? " (configured)" : " (new)") << "\n"
       << "# displayName: blank uses the route name\n"
       << "# api: blank keeps the catalog/default protocol\n"
       << "# models: comma-separated ids; blank keeps the catalog/default list\n"
       << "# baseURL: blank restores the provider default\n"
       << "# apiKey: blank keeps the current key; !clear removes the stored key\n";
  if (key_configured) seed << "# current key: configured " << masked << "\n";
  else seed << "# current key: not configured\n";
  seed << "\ndisplayName=" << (display_name == provider ? "" : display_name) << "\n"
       << "api=" << api << "\n"
       << "baseURL=" << base_url << "\n"
       << "models=" << models << "\n"
       << "apiKey=\n";
  const std::string original = seed.str();
  std::string text = original;
  if (!edit_in_screen(screen, text, state.notice)) return;
  if (text == original) {
    state.notice = "provider unchanged";
    return;
  }

  bool display_seen = false;
  bool api_seen = false;
  bool base_seen = false;
  bool models_seen = false;
  std::string next_display;
  std::string next_api;
  std::string next_base;
  std::vector<std::string> next_models;
  std::string api_key;
  std::istringstream input(text);
  std::string line;
  while (std::getline(input, line)) {
    const std::string_view view(line);
    if (view.rfind("displayName=", 0) == 0) {
      display_seen = true;
      next_display = value_after(view, "displayName=");
    } else if (view.rfind("api=", 0) == 0) {
      api_seen = true;
      next_api = value_after(view, "api=");
    } else if (view.rfind("baseURL=", 0) == 0) {
      base_seen = true;
      next_base = value_after(view, "baseURL=");
    } else if (view.rfind("models=", 0) == 0) {
      models_seen = true;
      next_models = split_models(value_after(view, "models="));
    } else if (view.rfind("apiKey=", 0) == 0) {
      api_key = value_after(view, "apiKey=");
    }
  }
  std::string next_model_ids;
  for (const auto& id : next_models) {
    if (!next_model_ids.empty()) next_model_ids += ", ";
    next_model_ids += id;
  }
  const std::string initial_display = display_name == provider ? std::string{} : display_name;
  nlohmann::json params = {{"provider", provider}};
  if (display_seen && (configured ? next_display != initial_display : !next_display.empty())) {
    params["displayName"] = next_display;
  }
  if (api_seen && (configured ? next_api != api : !next_api.empty())) params["api"] = next_api;
  if (base_seen && (configured ? next_base != base_url : !next_base.empty())) params["baseURL"] = next_base;
  if (models_seen && (configured ? next_model_ids != models : !next_models.empty())) {
    params["models"] = next_models;
  }
  if (!api_key.empty()) {
    if (api_key == "!clear") params["removeKey"] = true;
    else params["apiKey"] = api_key;
  }
  if (params.size() == 1) {
    state.notice = "provider unchanged";
    return;
  }
  auto saved = control.request("providers.update", params, std::chrono::seconds(60));
  state.notice = saved ? saved->value("text", std::string{"provider updated"}) : saved.error().detail;
  force_repaint(screen);
}

void discover_provider_models(ControlClient& control, ftxui::ScreenInteractive& screen,
                              AppState& state, std::string provider) {
  provider = trim(provider);
  if (provider.empty()) {
    state.notice = "usage: /providers models <provider>";
    return;
  }
  auto discovered = control.request("providers.discover", {{"provider", provider}},
                                    std::chrono::seconds(60));
  state.notice = discovered ? discovered->value("text", std::string{"no models discovered"})
                            : discovered.error().detail;
  force_repaint(screen);
}

void remove_provider(ControlClient& control, ftxui::ScreenInteractive& screen, AppState& state,
                     std::string provider, bool remove_key) {
  provider = trim(provider);
  if (provider.empty()) {
    state.notice = "usage: /providers remove <provider> [--key]";
    return;
  }
  auto removed = control.request("providers.remove",
                                 {{"provider", provider}, {"removeKey", remove_key}},
                                 std::chrono::seconds(60));
  state.notice = removed ? removed->value("text", std::string{"provider removed"})
                         : removed.error().detail;
  force_repaint(screen);
}

}  // namespace booth
