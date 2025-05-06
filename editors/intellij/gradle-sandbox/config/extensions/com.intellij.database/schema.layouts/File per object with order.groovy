import com.intellij.database.model.DasObjectWithSource
import com.intellij.database.model.DasSchemaChild

LAYOUT.baseName { ctx -> baseName(ctx.object) }
LAYOUT.fileName { ctx -> String.format("%03d-%s.sql", ctx.count, ctx.baseName) }


def baseName(obj) {
  for (def cur = obj; cur != null; cur = cur.dasParent) {
    if (storeSeparately(cur)) return sanitize(cur.name)
  }
  return sanitize(obj.name)
}

def storeSeparately(obj) {
  return obj instanceof DasObjectWithSource || obj instanceof DasSchemaChild
}

def sanitize(name) {
  return name.replace('/', 'slash')
}