#include "interaction/editor.hpp"

#include <string>
#include <utility>

namespace booth {
namespace {

std::string without_trailing_newline(std::string text) {
  if (!text.empty() && text.back() == '\n') text.pop_back();
  return text;
}

}  // namespace

void Editor::WriteVimRegister(std::string text, bool linewise, bool yank, char reg) {
  if (reg == '_') return;
  if (text.empty() && !linewise) return;
  std::string stored = std::move(text);
  if (linewise) stored += '\n';
  const auto remember = [&](std::string value) { RememberKill(std::move(value)); };
  const auto named = [&](char name, bool append) {
    const std::string previous = vim_registers_[name];
    if (append && !previous.empty()) {
      const bool previous_linewise = previous.back() == '\n';
      if (previous_linewise || linewise) {
        vim_registers_[name] = without_trailing_newline(previous) + "\n" + without_trailing_newline(stored) + "\n";
      } else {
        vim_registers_[name] = previous + stored;
      }
    } else {
      vim_registers_[name] = stored;
    }
  };
  if (reg == 0 || reg == '"') {
    if (yank) {
      vim_registers_['0'] = stored;
    } else if (linewise || stored.find('\n') != std::string::npos) {
      for (char digit = '9'; digit > '1'; --digit) {
        const auto shifted = vim_registers_.find(static_cast<char>(digit - 1));
        if (shifted != vim_registers_.end()) vim_registers_[digit] = shifted->second;
      }
      vim_registers_['1'] = stored;
    } else {
      vim_registers_['-'] = stored;
    }
  } else if (reg >= 'a' && reg <= 'z') {
    named(reg, false);
  } else if (reg >= 'A' && reg <= 'Z') {
    named(static_cast<char>(reg - 'A' + 'a'), true);
  } else if ((reg >= '0' && reg <= '9') || reg == '-' || reg == '+' || reg == '*') {
    vim_registers_[reg] = stored;
  } else {
    return;
  }
  remember(stored);
}

std::string Editor::ReadVimRegister(char reg) const {
  if (reg == '_') return {};
  if (reg == 0 || reg == '"') return kill_ring_.empty() ? std::string{} : kill_ring_.back();
  char name = reg;
  if (name >= 'A' && name <= 'Z') name = static_cast<char>(name - 'A' + 'a');
  const auto found = vim_registers_.find(name);
  return found == vim_registers_.end() ? std::string{} : found->second;
}

}  // namespace booth
