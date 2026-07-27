import QtQuick
import "."

// 视图模型错误文案展示（errorMessage 为 Sidecar 返回的中文用户可读文案）。
Text {
    property var vm: null

    visible: vm !== null && vm.errorMessage !== undefined && String(vm.errorMessage).length > 0
    text: (vm !== null && vm.errorMessage !== undefined) ? String(vm.errorMessage) : ""
    color: Theme.danger
    wrapMode: Text.Wrap
    font.pixelSize: 12
}
