"""编辑器公共装配入口。"""

from .document import EditorDocument
from .service import EditorService, OpenDocumentsModel


def create_editor_service(client) -> EditorService:
    return EditorService(client)


__all__ = ["EditorDocument", "EditorService", "OpenDocumentsModel", "create_editor_service"]
