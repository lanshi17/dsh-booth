#include "interaction/clipboard.hpp"

#include "interaction/editor.hpp"

#include <array>
#include <cstdio>
#include <cstdlib>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <optional>
#include <string>

namespace booth {
namespace {

std::string base64(const std::string& text) {
  static constexpr char alphabet[] =
      "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
  std::string encoded;
  std::size_t index = 0;
  while (index + 2 < text.size()) {
    const auto block = (static_cast<unsigned char>(text[index]) << 16) |
                       (static_cast<unsigned char>(text[index + 1]) << 8) |
                       static_cast<unsigned char>(text[index + 2]);
    encoded += alphabet[(block >> 18) & 63];
    encoded += alphabet[(block >> 12) & 63];
    encoded += alphabet[(block >> 6) & 63];
    encoded += alphabet[block & 63];
    index += 3;
  }
  const std::size_t rest = text.size() - index;
  if (rest == 1) {
    const auto block = static_cast<unsigned char>(text[index]) << 16;
    encoded += alphabet[(block >> 18) & 63];
    encoded += alphabet[(block >> 12) & 63];
    encoded += "==";
  } else if (rest == 2) {
    const auto block = (static_cast<unsigned char>(text[index]) << 16) |
                       (static_cast<unsigned char>(text[index + 1]) << 8);
    encoded += alphabet[(block >> 18) & 63];
    encoded += alphabet[(block >> 12) & 63];
    encoded += alphabet[(block >> 6) & 63];
    encoded += '=';
  }
  return encoded;
}

std::string capture(const std::string& command) {
  FILE* pipe = ::popen(command.c_str(), "r");
  if (pipe == nullptr) return {};
  std::string output;
  std::array<char, 8192> chunk{};
  std::size_t read = 0;
  while ((read = std::fread(chunk.data(), 1, chunk.size(), pipe)) > 0) {
    output.append(chunk.data(), read);
  }
  const int status = ::pclose(pipe);
  if (status != 0) return {};
  return output;
}

}  // namespace

std::optional<acp::ImageAttachment> clipboard_image_from_path(std::string_view path_text) {
  std::string path(path_text);
  const auto first = path.find_first_not_of(" \t\r\n");
  if (first == std::string::npos) return std::nullopt;
  const auto last = path.find_last_not_of(" \t\r\n");
  path = path.substr(first, last - first + 1);
  if (path.rfind("file://", 0) == 0) path.erase(0, 7);
  if (path.empty()) return std::nullopt;
  std::string lower = path;
  for (char& ch : lower) ch = static_cast<char>(std::tolower(static_cast<unsigned char>(ch)));
  std::string media;
  if (lower.ends_with(".png")) media = "image/png";
  else if (lower.ends_with(".jpg") || lower.ends_with(".jpeg")) media = "image/jpeg";
  else if (lower.ends_with(".webp")) media = "image/webp";
  else if (lower.ends_with(".gif")) media = "image/gif";
  else return std::nullopt;
  std::error_code error;
  if (!std::filesystem::is_regular_file(path, error)) return std::nullopt;
  const auto size = std::filesystem::file_size(path, error);
  if (error || size > 32 * 1024 * 1024) return std::nullopt;
  std::ifstream input(path, std::ios::binary);
  if (!input) return std::nullopt;
  const std::string data((std::istreambuf_iterator<char>(input)), std::istreambuf_iterator<char>());
  if (data.empty()) return std::nullopt;
  return acp::ImageAttachment{media, base64(data), std::filesystem::path(path).filename().string()};
}

std::optional<acp::ImageAttachment> clipboard_image() {
  static constexpr std::array<const char*, 4> kMedia{
      "image/png", "image/jpeg", "image/webp", "image/gif"};
  static constexpr std::array<const char*, 4> kSuffix{".png", ".jpg", ".webp", ".gif"};
  for (std::size_t index = 0; index < kMedia.size(); ++index) {
    const std::string media = kMedia[index];
    const std::string wayland = capture("wl-paste --no-newline --type " + media + " 2>/dev/null");
    const std::string x11 = wayland.empty()
                                ? capture("xclip -selection clipboard -t " + media + " -o 2>/dev/null")
                                : std::string{};
    const std::string& data = !wayland.empty() ? wayland : x11;
    if (data.empty() || data.size() > 32 * 1024 * 1024) continue;
    return acp::ImageAttachment{media, base64(data), "clipboard" + std::string(kSuffix[index])};
  }
  return std::nullopt;
}

std::optional<std::string> clipboard_text() {
  std::string text = capture("wl-paste --no-newline 2>/dev/null");
  if (text.empty()) text = capture("xclip -selection clipboard -o 2>/dev/null");
  if (text.empty() || text.size() > 1024 * 1024) return std::nullopt;
  return text;
}

void paste_clipboard(Editor& editor, std::vector<acp::ImageAttachment>& images, AppState& state) {
  if (auto image = clipboard_image()) {
    images.push_back(std::move(*image));
    state.notice = "image attached";
    return;
  }
  if (auto text = clipboard_text()) {
    if (auto image = clipboard_image_from_path(*text)) {
      images.push_back(std::move(*image));
      state.notice = "image attached";
      return;
    }
    editor.Paste(*text);
    state.notice = text->size() > 1000 ? "large text pasted" : "text pasted";
    return;
  }
  state.notice = "no image or text in clipboard";
}

std::string last_answer(const AppState& state) {
  for (auto entry = state.entries.rbegin(); entry != state.entries.rend(); ++entry) {
    if (entry->kind == EntryKind::Assistant) return entry->text;
  }
  return state.streaming;
}

std::string clipboard_sequence(const std::string& text) {
  const std::string osc = "\x1b]52;c;" + base64(text) + "\x07";
  const char* tmux = std::getenv("TMUX");
  const char* force = std::getenv("BOOTH_CLIPBOARD_TMUX");
  const char* term = std::getenv("TERM");
  const std::string term_name = term == nullptr ? "" : term;
  const bool ssh = std::getenv("SSH_CONNECTION") != nullptr || std::getenv("SSH_TTY") != nullptr ||
                   std::getenv("SSH_CLIENT") != nullptr;
  const bool nested = tmux != nullptr || (force != nullptr && *force != '\0' && *force != '0') ||
                      (ssh && (term_name.starts_with("screen") || term_name.starts_with("tmux")));
  if (!nested) return osc;
  return "\x1bPtmux;\x1b" + osc + "\x1b\\";
}

bool copy_to_clipboard(const std::string& text) {
  if (text.empty()) return false;
  std::cout << clipboard_sequence(text) << std::flush;
  return true;
}

void copy_answer(AppState& state) {
  const std::string text = last_answer(state);
  if (!copy_to_clipboard(text)) {
    state.notice = "nothing to copy";
    return;
  }
  state.notice = "copied " + std::to_string(text.size()) + " bytes";
}

}  // namespace booth
