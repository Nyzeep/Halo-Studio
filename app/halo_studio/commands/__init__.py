"""命令注册、when 上下文与快速打开匹配器。"""

from .registry import Command, CommandRegistry
from .when_context import WhenContext

__all__ = ["Command", "CommandRegistry", "WhenContext"]
