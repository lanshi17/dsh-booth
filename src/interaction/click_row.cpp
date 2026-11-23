#include "interaction/click_row.hpp"

#include <utility>

namespace booth {
namespace {

class ClickRowNode : public ftxui::Node {
 public:
  ClickRowNode(ftxui::Element child, std::size_t index, std::vector<ClickRow>* rows)
      : child_(std::move(child)), index_(index), rows_(rows) {}

  void ComputeRequirement() override {
    child_->ComputeRequirement();
    requirement_ = child_->requirement();
  }

  void SetBox(ftxui::Box box) override {
    child_->SetBox(box);
    rows_->push_back(ClickRow{index_, box.y_min, box.y_max, box.x_min, box.x_max});
    ftxui::Node::SetBox(box);
  }

  void Select(ftxui::Selection& selection) override { child_->Select(selection); }

  void Check(ftxui::Node::Status* status) override { child_->Check(status); }

  std::string GetSelectedContent(ftxui::Selection& selection) override {
    return child_->GetSelectedContent(selection);
  }

  void Render(ftxui::Screen& screen) override { child_->Render(screen); }

 private:
  ftxui::Element child_;
  std::size_t index_;
  std::vector<ClickRow>* rows_;
};

}  // namespace

ftxui::Element click_row(ftxui::Element child, std::size_t index, std::vector<ClickRow>* rows) {
  return std::make_shared<ClickRowNode>(std::move(child), index, rows);
}

}  // namespace booth
