#include "interaction/clipboard.hpp"

#include <cstdlib>
#include <iostream>
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

}  // namespace

std::string last_answer(const AppState& state) {
  for (auto entry = state.entries.rbegin(); entry != state.entries.rend(); ++entry) {
    if (entry->kind == EntryKind::Assistant) return entry->text;
  }
  return state.streaming;
}

std::string clipboard_sequence(const std::string& text) {
  const std::string osc = "\x1b]52;c;" + base64(text) + "\x07";
  if (std::getenv("TMUX") == nullptr) return osc;
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
