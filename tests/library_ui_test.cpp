#include <catch2/catch_test_macros.hpp>

#include <filesystem>
#include <fstream>
#include <string>
#include <vector>

#include <unistd.h>

#include <ftxui/dom/elements.hpp>
#include <ftxui/screen/screen.hpp>

#include "interaction/editor.hpp"
#include "interaction/library_ui.hpp"
#include "interaction/model_picker.hpp"

using namespace booth;

TEST_CASE("library overlay filters and selects a row") {
  LibraryOverlay overlay;
  overlay.open(LibraryKind::Skills, "skills",
               {{"review", "/review", "review a change"}, {"fix", "/fix", "fix a test"}});
  REQUIRE(overlay.visible());
  for (const char ch : std::string("fix")) {
    ftxui::Event event = ftxui::Event::Character(ch);
    overlay.key(event);
  }
  ftxui::Event enter = ftxui::Event::Return;
  REQUIRE(overlay.key(enter) == "fix");
  REQUIRE(overlay.visible());
  auto view = overlay.view();
  ftxui::Screen screen(70, 12);
  ftxui::Render(screen, std::move(view));
  REQUIRE(screen.ToString().find("skills") != std::string::npos);
}

TEST_CASE("skills selection fills the composer and providers open the editor") {
  AppState state;
  LibrarySettings settings;
  state.models = {{"alpha", "one", "a1"}, {"alpha", "two", "a2"}, {"beta", "three", "b1"}};
  LibraryOverlay overlay;
  Editor editor(state.composer);

  std::string content;
  Editor skill_editor(content);
  overlay.open(LibraryKind::Skills, "skills", {{"review", "/review", "review"}});
  ftxui::Event enter = ftxui::Event::Return;
  REQUIRE(handle_library_event(enter, state, overlay, skill_editor, settings));
  REQUIRE(state.composer == "/review ");
  REQUIRE(content == "/review ");

  std::string edited;
  settings.edit_provider = [&](const std::string& provider) { edited = provider; };
  REQUIRE(handle_library_command("/providers", state, overlay, ".", settings, editor));
  REQUIRE(overlay.visible());
  for (const char ch : std::string("alp")) {
    ftxui::Event event = ftxui::Event::Character(ch);
    overlay.key(event);
  }
  ftxui::Event provider_enter = ftxui::Event::Return;
  REQUIRE(handle_library_event(provider_enter, state, overlay, editor, settings));
  REQUIRE_FALSE(overlay.visible());
  REQUIRE(edited == "alpha");
}

TEST_CASE("provider overlay offers configuration fields") {
  AppState state;
  LibrarySettings settings;
  Editor editor(state.composer);
  state.models = {{"alpha", "one", "a1"}, {"alpha", "two", "a2"}, {"beta", "three", "b1"}};
  LibraryOverlay overlay;
  REQUIRE(handle_library_command("/providers", state, overlay, ".", settings, editor));
  auto view = overlay.view();
  ftxui::Screen screen(72, 14);
  ftxui::Render(screen, std::move(view));
  const std::string text = screen.ToString();
  REQUIRE(text.find("alpha") != std::string::npos);
  REQUIRE(text.find("live directory unavailable") != std::string::npos);
  REQUIRE(text.find("Provider configuration") != std::string::npos);
}

TEST_CASE("settings overlay toggles interaction state") {
  AppState state;
  bool thoughts = false;
  bool tools_hidden = false;
  bool expanded = false;
  bool mouse = false;
  LibrarySettings settings{&thoughts, &tools_hidden, &expanded, &mouse, nullptr,
                           std::filesystem::temp_directory_path(), nullptr};
  Editor editor(state.composer);
  LibraryOverlay overlay;
  REQUIRE(handle_library_command("/settings", state, overlay, ".", settings, editor));
  REQUIRE(overlay.visible());
  for (const char ch : std::string("thinking")) {
    ftxui::Event event = ftxui::Event::Character(ch);
    overlay.key(event);
  }
  ftxui::Event enter = ftxui::Event::Return;
  REQUIRE(handle_library_event(enter, state, overlay, editor, settings));
  REQUIRE(thoughts);
  REQUIRE(overlay.visible());

  for (const char ch : std::string("vim")) {
    ftxui::Event event = ftxui::Event::Character(ch);
    overlay.key(event);
  }
  REQUIRE(handle_library_event(enter, state, overlay, editor, settings));
  REQUIRE(editor.VimEnabled());
}

TEST_CASE("skills subcommands describe local roots without the registry") {
  const auto project = std::filesystem::temp_directory_path() /
                       ("booth-skills-cmd-" + std::to_string(::getpid()));
  std::filesystem::remove_all(project);
  std::filesystem::create_directories(project / ".git");
  std::filesystem::create_directories(project / ".dsh" / "skills" / "review");
  std::ofstream(project / ".dsh" / "skills" / "review" / "SKILL.md")
      << "---\nname: review\ndescription: review a change boothunique\n---\nReview.\n";
  AppState state;
  LibrarySettings settings;
  Editor editor(state.composer);
  LibraryOverlay overlay;
  REQUIRE(handle_library_command("/skills installed", state, overlay, project.string(), settings, editor));
  REQUIRE(state.notice.find("review") != std::string::npos);
  REQUIRE(handle_library_command("/skills search boothunique", state, overlay, project.string(), settings, editor));
  REQUIRE(state.notice.find("/review") != std::string::npos);
  REQUIRE(handle_library_command("/skills search zzzznothing", state, overlay, project.string(), settings, editor));
  REQUIRE(state.notice == "no matching skills");
  REQUIRE(handle_library_command("/skills install thing", state, overlay, project.string(), settings, editor));
  REQUIRE(state.notice.find("registry is unavailable") != std::string::npos);
  std::filesystem::remove_all(project);
}
