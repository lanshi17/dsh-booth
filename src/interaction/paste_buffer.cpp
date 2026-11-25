#include "interaction/paste_buffer.hpp"

#include "interaction/editor.hpp"

namespace booth {

bool PasteBuffer::feed(const ftxui::Event& event, Editor& editor) {
  if (event.input() == "\x1b[200~") {
    active_ = true;
    text_.clear();
    return true;
  }
  if (event.input() == "\x1b[201~") {
    active_ = false;
    if (!text_.empty()) editor.Paste(text_);
    text_.clear();
    return true;
  }
  if (!active_) return false;
  if (event.is_character()) {
    text_ += event.character();
  } else if (event == ftxui::Event::Return) {
    text_ += '\n';
  } else if (event == ftxui::Event::Tab) {
    text_ += '\t';
  }
  return true;
}

}  // namespace booth
