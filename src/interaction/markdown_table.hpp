#pragma once

#include <cstddef>
#include <string>
#include <vector>

#include <ftxui/dom/elements.hpp>

namespace booth {

struct TableCell {
  std::string text;
  std::string href;
  bool bold{false};
  bool code{false};
  bool italic{false};
};

enum class TableAlign { Left, Right, Center };

std::vector<std::string> table_cells(const std::string& line);
bool is_table_start(const std::vector<std::string>& lines, std::size_t index);
std::vector<TableAlign> table_alignments(const std::string& line);
std::vector<TableCell> styled_cells(const std::vector<std::string>& cells);
ftxui::Element table_view(const std::vector<std::vector<TableCell>>& rows,
                          const std::vector<TableAlign>& aligns, int max_width);

}  // namespace booth
