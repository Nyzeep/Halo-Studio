import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "../components"
import "../components/util.js" as Util

// 配置页：受管启动配置列表 + 表单。
// 凭据红线：本页只显示凭据引用名与存在性检查结果，永不出现密钥明文输入。
RowLayout {
    id: configPage

    readonly property var cVM: (typeof configVM !== "undefined") ? configVM : null
    property string editingConfigId: ""

    spacing: 10

    function loadRow(row) {
        if (cVM === null || cVM.configs === undefined || !cVM.configs || typeof cVM.configs.get !== "function")
            return
        var cfg = cVM.configs.get(row)
        if (!cfg)
            return
        editingConfigId = Util.textOr(cfg.config_id, "")
        cfgNameInput.text = Util.textOr(cfg.name, "")
        if (String(cfg.agent) === "opencode")
            cfgAgentOc.checked = true
        else
            cfgAgentPi.checked = true
        cfgExeInput.text = Util.textOr(cfg.executable_path, "")
        cfgModelInput.text = Util.textOr(cfg.model, "")
        var idx = cfgThinkingSelect.indexOfValue(String(cfg.thinking_level))
        cfgThinkingSelect.currentIndex = idx >= 0 ? idx : 0
        cfgCredRefInput.text = Util.textOr(cfg.credential_ref, "")
    }

    SectionCard {
        Layout.preferredWidth: 340
        Layout.fillHeight: true
        title: "受管启动配置"

        RowLayout {
            Layout.fillWidth: true
            Item { Layout.fillWidth: true }
            Button {
                text: "刷新列表"
                onClicked: if (configPage.cVM !== null && configPage.cVM.refresh) configPage.cVM.refresh()
            }
        }
        ListView {
            id: cfgList
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            spacing: 2
            model: (configPage.cVM !== null && configPage.cVM.configs !== undefined) ? configPage.cVM.configs : null
            ScrollBar.vertical: ScrollBar {}
            delegate: Rectangle {
                width: cfgList.width
                radius: Theme.radius
                color: ListView.isCurrentItem ? Theme.surfaceAlt : "transparent"
                implicitHeight: 44
                MouseArea {
                    anchors.fill: parent
                    onClicked: {
                        cfgList.currentIndex = index
                        configPage.loadRow(index)
                    }
                }
                RowLayout {
                    anchors.fill: parent
                    anchors.leftMargin: 8
                    anchors.rightMargin: 8
                    spacing: 6
                    StatusBadge { label: Util.agentLabel(model.agent); tone: Theme.accent }
                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 2
                        Text {
                            Layout.fillWidth: true
                            text: Util.textOr(model.name, "（未命名）")
                            color: Theme.text
                            elide: Text.ElideRight
                            font.pixelSize: 12
                        }
                        Text {
                            Layout.fillWidth: true
                            text: Util.textOr(model.model, "")
                            color: Theme.textDim
                            elide: Text.ElideRight
                            font.pixelSize: 11
                        }
                    }
                }
            }
        }
        Text {
            visible: cfgList.count === 0
            text: "暂无启动配置，请在右侧填写并保存。"
            color: Theme.textDim
            font.pixelSize: 12
        }
        Button {
            text: "删除所选配置"
            enabled: cfgList.currentIndex >= 0 && cfgList.count > 0
            onClicked: {
                if (configPage.cVM === null || configPage.cVM.configs === undefined)
                    return
                var cfg = configPage.cVM.configs.get(cfgList.currentIndex)
                if (cfg && Util.hasText(cfg.config_id))
                    configPage.cVM["delete"](String(cfg.config_id))
            }
        }
    }

    SectionCard {
        Layout.fillWidth: true
        Layout.fillHeight: true
        title: "配置表单"

        RowLayout {
            Layout.fillWidth: true
            spacing: 8
            Text { text: "名称："; color: Theme.textDim; font.pixelSize: 12 }
            TextField { id: cfgNameInput; Layout.fillWidth: true; placeholderText: "如：Pi + GPT" }
        }
        RowLayout {
            spacing: 8
            Text { text: "Agent："; color: Theme.textDim; font.pixelSize: 12 }
            RadioButton { id: cfgAgentPi; text: "Pi"; checked: true }
            RadioButton { id: cfgAgentOc; text: "OpenCode" }
        }
        RowLayout {
            Layout.fillWidth: true
            spacing: 8
            Text { text: "可执行文件："; color: Theme.textDim; font.pixelSize: 12 }
            TextField { id: cfgExeInput; Layout.fillWidth: true; placeholderText: "C:\\tools\\pi\\pi.exe" }
        }
        RowLayout {
            Layout.fillWidth: true
            spacing: 8
            Text { text: "模型："; color: Theme.textDim; font.pixelSize: 12 }
            TextField { id: cfgModelInput; Layout.fillWidth: true; placeholderText: "如：gpt-5" }
        }
        RowLayout {
            Layout.fillWidth: true
            spacing: 8
            Text { text: "思考级别："; color: Theme.textDim; font.pixelSize: 12 }
            ComboBox {
                id: cfgThinkingSelect
                Layout.fillWidth: true
                textRole: "label"
                valueRole: "value"
                model: [
                    { label: "关闭", value: "off" },
                    { label: "低", value: "low" },
                    { label: "中", value: "medium" },
                    { label: "高", value: "high" }
                ]
            }
        }
        RowLayout {
            Layout.fillWidth: true
            spacing: 8
            Text { text: "凭据引用名："; color: Theme.textDim; font.pixelSize: 12 }
            TextField {
                id: cfgCredRefInput
                Layout.fillWidth: true
                placeholderText: "如 halo/pi/openai（仅引用名，非密钥）"
            }
            Button {
                text: "检查存在性"
                enabled: cfgCredRefInput.text.trim().length > 0
                onClicked: {
                    if (configPage.cVM !== null && configPage.cVM.credentialCheck)
                        configPage.cVM.credentialCheck(cfgCredRefInput.text.trim())
                }
            }
        }
        Text {
            Layout.fillWidth: true
            text: {
                if (configPage.cVM === null)
                    return "凭据检查结果：—"
                if (!Util.hasText(configPage.cVM.credentialCheckedRef))
                    return "凭据检查结果：尚未检查"
                var prefix = "凭据检查结果（" + configPage.cVM.credentialCheckedRef + "）："
                if (configPage.cVM.credentialStoreAvailable === false)
                    return prefix + "操作系统凭据存储不可用（失败关闭，不回退明文）"
                return prefix + (configPage.cVM.credentialExists === true ? "引用存在" : "引用不存在")
            }
            color: Theme.textDim
            wrapMode: Text.Wrap
            font.pixelSize: 12
        }
        Text {
            Layout.fillWidth: true
            text: "说明：界面只保存与显示凭据引用名。密钥请在命令行执行 halo-sidecar cred set <引用名> 录入；凭据明文不会出现在界面、日志或本地数据中。"
            color: Theme.textDim
            wrapMode: Text.Wrap
            font.pixelSize: 12
        }
        RowLayout {
            spacing: 8
            Button {
                text: configPage.editingConfigId.length > 0 ? "保存修改" : "保存为新配置"
                enabled: cfgNameInput.text.trim().length > 0 && cfgExeInput.text.trim().length > 0
                onClicked: {
                    if (configPage.cVM === null || !configPage.cVM.save)
                        return
                    var payload = {
                        "name": cfgNameInput.text.trim(),
                        "agent": cfgAgentPi.checked ? "pi" : "opencode",
                        "executable_path": cfgExeInput.text.trim(),
                        "model": cfgModelInput.text.trim(),
                        "thinking_level": (cfgThinkingSelect.currentValue === undefined || cfgThinkingSelect.currentValue === null)
                            ? "off" : String(cfgThinkingSelect.currentValue),
                        "credential_ref": cfgCredRefInput.text.trim().length > 0 ? cfgCredRefInput.text.trim() : null,
                        "extra_args": [],
                        "env_overrides": {}
                    }
                    if (configPage.editingConfigId.length > 0)
                        payload["config_id"] = configPage.editingConfigId
                    configPage.cVM.save(payload)
                }
            }
            Button {
                text: "清空表单（新建）"
                onClicked: {
                    configPage.editingConfigId = ""
                    cfgNameInput.text = ""
                    cfgAgentPi.checked = true
                    cfgExeInput.text = ""
                    cfgModelInput.text = ""
                    cfgThinkingSelect.currentIndex = 0
                    cfgCredRefInput.text = ""
                }
            }
        }
        ErrorLabel { Layout.fillWidth: true; vm: configPage.cVM }
        Item { Layout.fillHeight: true }
    }
}
