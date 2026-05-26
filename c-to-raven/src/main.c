#include <raven/raven.h>

typedef short int  bool;

typedef struct
{
    int x;
    int y;
} Vec2;

typedef struct Shape
{

    char *name;

    //* abstract methods

    int (*area)(void *self);
    void (*move)(void *self, Vec2 vector);

} Shape;

Shape __shape_init__(char *name, int (*area)(void *), void (*move)(void *, Vec2))
{
    return (Shape){
        .name = name,
        .area = area,
        .move = move};
}

bool validate_shape(Shape *shape)
{
    return shape->area != NULL && shape->move != NULL;
}

typedef struct Rect
{
    Shape shape;
    Vec2 position[4]; // top-left, top-right, bottom-right, bottom-left
    int (*width)(void *self);
    int (*height)(void *self);
} Rect;

int rect_width(void *self)
{
    Rect *rect = (Rect *)self;
    return rect->position[1].x - rect->position[0].x;
}

int rect_height(void *self)
{
    Rect *rect = (Rect *)self;
    return rect->position[2].y - rect->position[0].y;
}

int rect_area(void *self)
{
    Rect *rect = (Rect *)self;
    return rect_width(rect) * rect_height(rect);
}

void rect_move(void *self, Vec2 vector)
{
    Rect *rect = (Rect *)self;
    for (int i = 0; i < 4; i++)
    {
        rect->position[i].x += vector.x;
        rect->position[i].y += vector.y;
    }
    return;
}

Rect *__rect_init__(Vec2 position[4], char * name)
{
    name = name ? name : "Rectangle";
    Rect *rect = (Rect *)raven_malloc(sizeof(Rect));
    rect->shape = __shape_init__(name, rect_area, rect_move);
    for (int i = 0; i < 4; i++)
    {
        rect->position[i] = position[i];
    }
    rect->width = rect_width; // for squares, we would call replacing width and height with the same function, so we can use the ternary operator to check if width and height are provided, if not we can use the default rect_width and rect_height functions.
    rect->height = rect_height;
    return rect;
}

typedef struct Circle
{
    Shape shape;
    Vec2 center;
    int radius;
} Circle;

int circle_area(void *self)
{
    Circle *circle = (Circle *)self;
    return 3.14f * circle->radius * circle->radius;
}

void circle_move(void *self, Vec2 vector)
{
    Circle *circle = (Circle *)self;
    circle->center.x += vector.x;
    circle->center.y += vector.y;
    // Assuming the circle's position is defined by its center, we can move it by updating the center's coordinates.
    // However, since we don't have a specific position for the circle in this implementation, we will just return 0 for now.
    return;
}

Circle *__circle_init__(int radius, Vec2 center)
{
    Circle *circle = (Circle *)raven_malloc(sizeof(Circle));
    circle->shape = __shape_init__("Circle", circle_area, circle_move); // area and move can be implemented similarly to Rect
    circle->radius = radius;
    circle->center = center;
    return circle;
}

typedef struct Square
{
    Rect rect;
} Square;

Square *__square_init__(int lenght, Vec2 position)
{
    Vec2 positions[4] = {
        {.x = position.x, .y = position.y},
        {.x = position.x + lenght, .y = position.y},
        {.x = position.x, .y = position.y + lenght},
        {.x = position.x + lenght, .y = position.y + lenght}};
    return (Square *)__rect_init__(positions, "Square"); // since width and height are the same for a square, we can pass NULL for both and use the default rect_width and rect_height functions.
}

int main()
{
    Circle *circle = __circle_init__(5, (Vec2){.x = 0, .y = 0});

    Rect *rect = __rect_init__((Vec2[4]){
        {.x = 0, .y = 0},
        {.x = 4, .y = 0},
        {.x = 4, .y = 3},
        {.x = 0, .y = 3}}, NULL);

    Square *square = __square_init__(4, (Vec2){.x = 0, .y = 0});

    Shape *shapes[3] = {(Shape *)circle, (Shape *)rect, (Shape *)square};

    for (int i = 0; i < 3; i++)
    {
        if (validate_shape(shapes[i]))
        {
            raven_printf("%s area: %d\n", shapes[i]->name, shapes[i]->area(shapes[i]) );
        }
        else
        {
            raven_printf("%s is not a valid shape.\n", shapes[i]->name);
        }
    }

    return 0;
}
